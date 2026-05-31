//! The serializable draw-list render model and its backend (spec §24.6).
//!
//! This is the second output backend. It consumes the same planned
//! [`RenderScene`] as the SVG backend — never the source AST — and produces a
//! [`DrawList`]: a flat, deterministic sequence of drawable primitives. A
//! Canvas, raster, or WebGL client can replay the list without an SVG parser and
//! without a browser runtime.
//!
//! As of v0.29.0 the draw list is a *complete* scene description: the chart
//! frame (canvas, background, plot panels, facet strips, chart text) plus a
//! per-datum op for every geometry mark the SVG backend draws. Geometry emission
//! is shared between the two backends through the [`MarkSink`](crate::sink)
//! seam, so coordinates and colors agree by construction. Guide primitives
//! (axes, gridlines, legends) are emitted through the same seam during document
//! assembly. The draw list remains inert data: no scripts, no embedded behavior.

use std::fmt::Write as _;

use algraf_core::Diagnostic;

use crate::sink::{json_string, DrawListSink, Paint};
use crate::svg::num;

use super::backend::{RenderBackend, RenderScene};
use super::metadata::InteractionMetadata;
use super::panels::panel_slots;

/// Where a [`DrawOp`] sits in the chart, so clients and tests can identify
/// primitives without re-deriving them from coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawRole {
    /// The full-canvas background fill.
    Background,
    /// A plot area (one per space; one per facet panel when faceted).
    PlotArea,
    /// A facet strip background.
    FacetStrip,
    /// A facet strip label.
    FacetLabel,
    /// The chart title.
    Title,
    /// The chart subtitle.
    Subtitle,
    /// The chart caption.
    Caption,
    /// A per-datum geometry mark.
    Mark,
    /// A gridline (Cartesian or polar).
    Grid,
    /// An axis line, tick, tick label, or axis title.
    Axis,
    /// A legend swatch or label.
    Legend,
    /// A polar perimeter or radius label.
    PolarLabel,
}

impl DrawRole {
    /// The stable string used in the serialized draw list.
    pub fn as_str(self) -> &'static str {
        match self {
            DrawRole::Background => "background",
            DrawRole::PlotArea => "plot-area",
            DrawRole::FacetStrip => "facet-strip",
            DrawRole::FacetLabel => "facet-label",
            DrawRole::Title => "title",
            DrawRole::Subtitle => "subtitle",
            DrawRole::Caption => "caption",
            DrawRole::Mark => "mark",
            DrawRole::Grid => "grid",
            DrawRole::Axis => "axis",
            DrawRole::Legend => "legend",
            DrawRole::PolarLabel => "polar-label",
        }
    }
}

/// Horizontal text alignment for a [`DrawOp::Text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

impl TextAnchor {
    pub fn as_str(self) -> &'static str {
        match self {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }
}

/// A single drawable primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    /// Start a rectangular clip scope. All following primitives until the
    /// matching `ClipEnd` are clipped to this rectangle.
    ClipStart {
        role: DrawRole,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// End the current clip scope.
    ClipEnd { role: DrawRole },
    /// A rectangle.
    Rect {
        role: DrawRole,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        paint: Paint,
        /// Inert per-mark interaction metadata (spec §14.25, §24.6).
        interaction: Option<crate::sink::MarkInteraction>,
    },
    /// A circle.
    Circle {
        role: DrawRole,
        cx: f64,
        cy: f64,
        r: f64,
        paint: Paint,
        interaction: Option<crate::sink::MarkInteraction>,
    },
    /// A path, with an SVG path `d` mini-language string (M/L/A/Z commands).
    Path {
        role: DrawRole,
        d: String,
        paint: Paint,
        dash: Option<crate::sink::Dash>,
        interaction: Option<crate::sink::MarkInteraction>,
    },
    /// A polygon, with a space-separated `x,y` point list.
    Polygon {
        role: DrawRole,
        points: String,
        paint: Paint,
        interaction: Option<crate::sink::MarkInteraction>,
    },
    /// A single line segment.
    Line {
        role: DrawRole,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: String,
        stroke_width: f64,
        linecap_round: bool,
        opacity: Option<f64>,
        dash: Option<crate::sink::Dash>,
    },
    /// A run of text. `content` may contain `\n` for stacked lines.
    Text {
        role: DrawRole,
        x: f64,
        y: f64,
        anchor: TextAnchor,
        fill: String,
        opacity: Option<f64>,
        content: String,
    },
}

impl DrawOp {
    fn role(&self) -> DrawRole {
        match self {
            DrawOp::ClipStart { role, .. } | DrawOp::ClipEnd { role } => *role,
            DrawOp::Rect { role, .. }
            | DrawOp::Circle { role, .. }
            | DrawOp::Path { role, .. }
            | DrawOp::Polygon { role, .. }
            | DrawOp::Line { role, .. }
            | DrawOp::Text { role, .. } => *role,
        }
    }
}

/// A deterministic, drawable description of a chart (spec §24.6).
///
/// Produced by [`DrawListBackend`] from a planned [`RenderScene`].
#[derive(Debug, Clone, PartialEq)]
pub struct DrawList {
    pub width: f64,
    pub height: f64,
    pub interactions: InteractionMetadata,
    pub ops: Vec<DrawOp>,
}

impl DrawList {
    /// Serialize to deterministic JSON.
    ///
    /// Numbers use the same locale-independent formatting as SVG output
    /// (spec §18.8), so the draw list is byte-stable across platforms.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        let _ = write!(
            out,
            "\"width\":{},\"height\":{},\"interactions\":{},",
            num(self.width),
            num(self.height),
            self.interactions.to_json()
        );
        out.push_str("\"ops\":[");
        for (i, op) in self.ops.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            op.write_json(&mut out);
        }
        out.push_str("]}");
        out
    }
}

impl DrawOp {
    fn write_json(&self, out: &mut String) {
        let role = self.role().as_str();
        match self {
            DrawOp::ClipStart {
                x,
                y,
                width,
                height,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"clipStart\",\"role\":\"{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                    role,
                    num(*x),
                    num(*y),
                    num(*width),
                    num(*height),
                );
            }
            DrawOp::ClipEnd { .. } => {
                let _ = write!(out, "{{\"op\":\"clipEnd\",\"role\":\"{}\"}}", role);
            }
            DrawOp::Rect {
                x,
                y,
                width,
                height,
                paint,
                interaction,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"rect\",\"role\":\"{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}",
                    role,
                    num(*x),
                    num(*y),
                    num(*width),
                    num(*height),
                );
                paint.write_json(out);
                write_interaction_json(out, interaction);
                out.push('}');
            }
            DrawOp::Circle {
                cx,
                cy,
                r,
                paint,
                interaction,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"circle\",\"role\":\"{}\",\"cx\":{},\"cy\":{},\"r\":{}",
                    role,
                    num(*cx),
                    num(*cy),
                    num(*r),
                );
                paint.write_json(out);
                write_interaction_json(out, interaction);
                out.push('}');
            }
            DrawOp::Path {
                d,
                paint,
                dash,
                interaction,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"path\",\"role\":\"{}\",\"d\":{}",
                    role,
                    json_string(d),
                );
                paint.write_json(out);
                if let Some(dash) = dash {
                    let _ = write!(out, ",\"strokeDasharray\":\"{}\"", dash.dasharray());
                }
                write_interaction_json(out, interaction);
                out.push('}');
            }
            DrawOp::Polygon {
                points,
                paint,
                interaction,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"polygon\",\"role\":\"{}\",\"points\":{}",
                    role,
                    json_string(points),
                );
                paint.write_json(out);
                write_interaction_json(out, interaction);
                out.push('}');
            }
            DrawOp::Line {
                x1,
                y1,
                x2,
                y2,
                stroke,
                stroke_width,
                linecap_round,
                opacity,
                dash,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"line\",\"role\":\"{}\",\"x1\":{},\"y1\":{},\"x2\":{},\"y2\":{},\"stroke\":{},\"strokeWidth\":{}",
                    role,
                    num(*x1),
                    num(*y1),
                    num(*x2),
                    num(*y2),
                    json_string(stroke),
                    num(*stroke_width),
                );
                if *linecap_round {
                    out.push_str(",\"strokeLinecap\":\"round\"");
                }
                if let Some(opacity) = opacity {
                    let _ = write!(out, ",\"opacity\":{}", num(*opacity));
                }
                if let Some(dash) = dash {
                    let _ = write!(out, ",\"strokeDasharray\":\"{}\"", dash.dasharray());
                }
                out.push('}');
            }
            DrawOp::Text {
                x,
                y,
                anchor,
                fill,
                opacity,
                content,
                ..
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"text\",\"role\":\"{}\",\"x\":{},\"y\":{},\"anchor\":\"{}\",\"fill\":{}",
                    role,
                    num(*x),
                    num(*y),
                    anchor.as_str(),
                    json_string(fill),
                );
                if let Some(opacity) = opacity {
                    let _ = write!(out, ",\"opacity\":{}", num(*opacity));
                }
                let _ = write!(out, ",\"content\":{}}}", json_string(content));
            }
        }
    }
}

/// Append a mark's inert interaction metadata to a draw-list op object, when
/// present (spec §14.25, §24.6). Emits a nested `"interaction"` object with
/// optional `tooltip` and `highlight` fields; absent when the mark carries none.
fn write_interaction_json(out: &mut String, interaction: &Option<crate::sink::MarkInteraction>) {
    let Some(mark) = interaction else {
        return;
    };
    if mark.is_empty() {
        return;
    }
    out.push_str(",\"interaction\":{");
    let mut first = true;
    if let Some(tooltip) = &mark.tooltip {
        let _ = write!(out, "\"tooltip\":{}", json_string(tooltip));
        first = false;
    }
    if let Some(highlight) = &mark.highlight {
        if !first {
            out.push(',');
        }
        let _ = write!(out, "\"highlight\":{}", json_string(highlight));
    }
    out.push('}');
}

/// The draw-list backend: walks a planned scene and records every primitive the
/// SVG backend draws (spec §24.6).
pub(super) struct DrawListBackend;

impl RenderBackend for DrawListBackend {
    type Output = DrawList;

    fn emit(
        &self,
        scene: &RenderScene<'_>,
        metadata: &InteractionMetadata,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> DrawList {
        let RenderScene {
            ir,
            layout,
            legends,
            panels,
            theme,
        } = *scene;
        let width = ir.width as f64;
        let height = ir.height as f64;
        let mut ops = Vec::new();

        // Background (mirrors document.rs step order: background first).
        ops.push(DrawOp::Rect {
            role: DrawRole::Background,
            x: 0.0,
            y: 0.0,
            width,
            height,
            paint: Paint::fill(theme.background.clone(), None),
            interaction: None,
        });

        // Chart title/subtitle/caption, using the same coordinates as the SVG
        // backend's render_chart_text.
        let text_x = layout.plot.x;
        let mut text_y = 24.0;
        if let Some(title) = &ir.title {
            ops.push(DrawOp::Text {
                role: DrawRole::Title,
                x: text_x,
                y: text_y,
                anchor: TextAnchor::Start,
                fill: theme.text_color.clone(),
                opacity: None,
                content: title.clone(),
            });
            text_y += theme.title_size + 8.0;
        }
        if let Some(subtitle) = &ir.subtitle {
            ops.push(DrawOp::Text {
                role: DrawRole::Subtitle,
                x: text_x,
                y: text_y,
                anchor: TextAnchor::Start,
                fill: theme.text_color.clone(),
                opacity: None,
                content: subtitle.clone(),
            });
        }

        // Plot areas and facet strips, in scene order.
        let slots = panel_slots(layout, panels);
        for slot in &slots {
            ops.push(DrawOp::Rect {
                role: DrawRole::PlotArea,
                x: slot.plot.x,
                y: slot.plot.y,
                width: slot.plot.width,
                height: slot.plot.height,
                paint: Paint::fill(theme.plot_background.clone(), None),
                interaction: None,
            });
        }
        for slot in &slots {
            if let (Some(strip), Some(panel)) = (slot.strip, slot.panel) {
                if strip.height <= 0.0 {
                    continue;
                }
                // Mirror guide::render_facet_label: strip uses the plot
                // background, the label baseline sits 4px above the strip bottom.
                ops.push(DrawOp::Rect {
                    role: DrawRole::FacetStrip,
                    x: strip.x,
                    y: strip.y,
                    width: strip.width,
                    height: strip.height,
                    paint: Paint::fill(panel.theme.plot_background.clone(), None),
                    interaction: None,
                });
                ops.push(DrawOp::Text {
                    role: DrawRole::FacetLabel,
                    x: strip.x + strip.width / 2.0,
                    y: strip.y + strip.height - 4.0,
                    anchor: TextAnchor::Middle,
                    fill: panel.theme.text_color.clone(),
                    opacity: None,
                    content: slot.label.unwrap_or_default().to_string(),
                });
            }
        }

        // Gridlines, then per-datum geometry marks, then axes/polar labels and
        // legends — the same walk and order the SVG backend uses, recorded
        // through the shared mark sink (spec §24.6).
        let mut sink = DrawListSink::new();
        super::document::paint_grid(&mut sink, &slots);
        super::document::paint_geometries(&mut sink, panels, diagnostics);
        super::document::paint_axes_and_legends(&mut sink, &slots, legends, layout, theme);
        ops.extend(sink.into_ops());

        if let Some(caption) = &ir.caption {
            ops.push(DrawOp::Text {
                role: DrawRole::Caption,
                x: width - 16.0,
                y: height - 12.0,
                anchor: TextAnchor::End,
                fill: theme.text_color.clone(),
                opacity: None,
                content: caption.clone(),
            });
        }

        DrawList {
            width,
            height,
            interactions: metadata.clone(),
            ops,
        }
    }
}
