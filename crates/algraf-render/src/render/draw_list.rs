//! The serializable draw-list render model and its backend (spec §24.6).
//!
//! This is the second output backend, added in v0.24.0 to prove that the render
//! execution boundary of §24.6 can drive more than one output format. It
//! consumes the same planned [`RenderScene`] as the SVG backend — never the
//! source AST — and produces a [`DrawList`]: a flat, deterministic sequence of
//! Canvas-drawable primitives (filled rectangles and text). A Canvas, raster, or
//! WebGL client can replay the list without an SVG parser and without a browser
//! runtime.
//!
//! # Documented equivalence limits (capstone, v0.24.0)
//!
//! The draw list covers the chart *frame* that planning resolves directly from
//! the scene: canvas size, the chart background, each plot panel (and its facet
//! strip and label), and the chart title/subtitle/caption. It deliberately does
//! **not** yet include per-datum geometry marks (points, bars, lines, areas),
//! axis ticks, or gridlines: those are still emitted as raw SVG by [`crate::geom`]
//! and [`crate::guide`] and would require routing geometry emission through a
//! shared primitive sink. Promoting full mark/guide parity is tracked as the
//! follow-up to the v0.24 backend work. Coordinates and colors that the draw list
//! does emit match the SVG backend exactly, so the two backends agree on canvas
//! dimensions, background, and panel placement.

use std::fmt::Write as _;

use algraf_core::Diagnostic;

use crate::svg::num;

use super::backend::{RenderBackend, RenderScene};
use super::panels::panel_slots;

/// Where a [`DrawOp`] sits in the chart frame, so clients and tests can identify
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

/// A single Canvas-drawable primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    /// A filled rectangle (`ctx.fillRect` after setting `fillStyle`).
    Rect {
        role: DrawRole,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: String,
    },
    /// A run of text (`ctx.fillText` after setting `fillStyle`/`textAlign`).
    Text {
        role: DrawRole,
        x: f64,
        y: f64,
        anchor: TextAnchor,
        fill: String,
        content: String,
    },
}

/// A deterministic, Canvas-drawable description of a chart's frame (spec §24.6).
///
/// Produced by [`DrawListBackend`] from a planned [`RenderScene`]. See the module
/// docs for the documented equivalence limits relative to the SVG backend.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawList {
    pub width: f64,
    pub height: f64,
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
            "\"width\":{},\"height\":{},",
            num(self.width),
            num(self.height)
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
        match self {
            DrawOp::Rect {
                role,
                x,
                y,
                width,
                height,
                fill,
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"rect\",\"role\":\"{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"fill\":{}}}",
                    role.as_str(),
                    num(*x),
                    num(*y),
                    num(*width),
                    num(*height),
                    json_string(fill),
                );
            }
            DrawOp::Text {
                role,
                x,
                y,
                anchor,
                fill,
                content,
            } => {
                let _ = write!(
                    out,
                    "{{\"op\":\"text\",\"role\":\"{}\",\"x\":{},\"y\":{},\"anchor\":\"{}\",\"fill\":{},\"content\":{}}}",
                    role.as_str(),
                    num(*x),
                    num(*y),
                    anchor.as_str(),
                    json_string(fill),
                    json_string(content),
                );
            }
        }
    }
}

/// Escape a string as a JSON string literal (including surrounding quotes).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The draw-list backend: walks a planned scene and records frame primitives.
pub(super) struct DrawListBackend;

impl RenderBackend for DrawListBackend {
    type Output = DrawList;

    fn emit(&self, scene: &RenderScene<'_>, _diagnostics: &mut Vec<Diagnostic>) -> DrawList {
        let RenderScene {
            ir,
            layout,
            panels,
            theme,
            ..
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
            fill: theme.background.clone(),
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
                fill: theme.plot_background.clone(),
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
                    fill: panel.theme.plot_background.clone(),
                });
                ops.push(DrawOp::Text {
                    role: DrawRole::FacetLabel,
                    x: strip.x + strip.width / 2.0,
                    y: strip.y + strip.height - 4.0,
                    anchor: TextAnchor::Middle,
                    fill: panel.theme.text_color.clone(),
                    content: slot.label.unwrap_or_default().to_string(),
                });
            }
        }

        if let Some(caption) = &ir.caption {
            ops.push(DrawOp::Text {
                role: DrawRole::Caption,
                x: width - 16.0,
                y: height - 12.0,
                anchor: TextAnchor::End,
                fill: theme.text_color.clone(),
                content: caption.clone(),
            });
        }

        DrawList { width, height, ops }
    }
}
