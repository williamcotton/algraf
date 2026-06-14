//! The mark sink: a backend-neutral primitive sink shared by geometry and guide
//! emission (spec §24.6).
//!
//! Geometry and guide code does not know which backend it draws into. Instead of
//! formatting SVG strings inline, it describes each primitive — rectangles,
//! circles, paths, polygons, lines, and text — by calling a [`MarkSink`]. Two
//! sinks consume those calls:
//!
//! - [`SvgSink`] wraps an [`SvgWriter`] and reproduces the canonical SVG byte for
//!   byte (spec §18). It is the regression baseline.
//! - [`DrawListSink`] records the same primitives as serializable
//!   [`DrawOp`](crate::render::DrawOp)s for the draw-list backend, so the draw
//!   list gains a per-datum op for every element the SVG backend draws.
//!
//! Because both sinks see the same calls, the two backends agree on coordinates
//! and colors by construction (spec §24.6).

use algraf_semantics::{FontStyleIr, FontWeightIr};

use crate::layout::Rect;
use crate::render::{DrawOp, DrawRole, TextAnchor};
use crate::svg::{escape_attr, escape_text, num, SvgWriter};

/// A stroke dash pattern (spec §14.x). Mirrors the SVG `stroke-dasharray`
/// presets used by annotation lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dash {
    Dotted,
    Dashed,
}

impl Dash {
    pub fn from_setting(value: Option<&str>) -> Option<Self> {
        match value {
            Some("dotted") => Some(Dash::Dotted),
            Some("dashed") => Some(Dash::Dashed),
            _ => None,
        }
    }

    /// The SVG `stroke-dasharray` value for this preset.
    pub fn dasharray(self) -> &'static str {
        match self {
            Dash::Dotted => "1 2",
            Dash::Dashed => "4 4",
        }
    }
}

/// The fill of a shape primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    /// No fill (`fill="none"`).
    None,
    /// A solid color.
    Color(String),
}

/// The stroke of a shape primitive. The three states are byte-distinct in SVG:
/// `Omit` writes no `stroke` attribute, `None` writes `stroke="none"`, and
/// `Solid` writes `stroke`/`stroke-width`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stroke {
    Omit,
    None,
    Solid { color: String, width: f64 },
}

/// The paint of a shape primitive (rectangle, circle, path, polygon).
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub fill: Fill,
    pub stroke: Stroke,
    pub opacity: Option<f64>,
}

impl Paint {
    pub(crate) fn fill(color: impl Into<String>, opacity: Option<f64>) -> Self {
        Paint {
            fill: Fill::Color(color.into()),
            stroke: Stroke::Omit,
            opacity,
        }
    }

    /// Append this paint's SVG attribute tail (`fill`, optional `stroke`/
    /// `stroke-width`, optional `opacity`) in the canonical order (spec §18).
    pub(crate) fn write_svg(&self, out: &mut String) {
        match &self.fill {
            Fill::None => out.push_str(" fill=\"none\""),
            Fill::Color(c) => {
                out.push_str(" fill=\"");
                out.push_str(&escape_attr(c));
                out.push('"');
            }
        }
        match &self.stroke {
            Stroke::Omit => {}
            Stroke::None => out.push_str(" stroke=\"none\""),
            Stroke::Solid { color, width } => {
                out.push_str(" stroke=\"");
                out.push_str(&escape_attr(color));
                out.push_str("\" stroke-width=\"");
                out.push_str(&num(width.max(0.0)));
                out.push('"');
            }
        }
        if let Some(opacity) = self.opacity {
            out.push_str(" opacity=\"");
            out.push_str(&num(opacity));
            out.push('"');
        }
    }

    /// Append this paint's JSON fields (`fill`, optional `stroke`/`strokeWidth`/
    /// `opacity`) to a draw-list object.
    pub(crate) fn write_json(&self, out: &mut String) {
        use std::fmt::Write as _;
        match &self.fill {
            Fill::None => out.push_str(",\"fill\":\"none\""),
            Fill::Color(c) => {
                let _ = write!(out, ",\"fill\":{}", json_string(c));
            }
        }
        if let Stroke::Solid { color, width } = &self.stroke {
            let _ = write!(
                out,
                ",\"stroke\":{},\"strokeWidth\":{}",
                json_string(color),
                num(width.max(0.0))
            );
        } else if matches!(self.stroke, Stroke::None) {
            out.push_str(",\"stroke\":\"none\"");
        }
        if let Some(opacity) = self.opacity {
            let _ = write!(out, ",\"opacity\":{}", num(opacity));
        }
    }
}

/// Declarative interaction metadata attached to a per-datum mark (spec §14.25,
/// §24.6).
///
/// This is inert data: the SVG backend turns it into an accessible `<title>`
/// tooltip and a stable highlight-group attribute, and the draw-list backend
/// records it verbatim. There is no script and no behavior — a viewer (the
/// opt-in interactive runtime or a Canvas host) interprets it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkInteraction {
    /// Tooltip text (newline-separated `label: value` lines), if any.
    pub tooltip: Option<String>,
    /// The highlight group value this mark belongs to, if any.
    pub highlight: Option<String>,
    /// Optional host-side event emitter metadata.
    pub event: Option<MarkEventInteraction>,
}

/// Inert event metadata attached to one per-datum mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkEventInteraction {
    pub event: String,
    pub emit_field: String,
    pub value: Option<String>,
}

impl MarkInteraction {
    pub(crate) fn is_empty(&self) -> bool {
        self.tooltip.is_none() && self.highlight.is_none() && self.event.is_none()
    }
}

/// One run of text to draw. Mirrors the SVG `<text>` attribute set used across
/// geometry and guide emission, in the canonical attribute order (spec §18).
pub(crate) struct TextRun<'a> {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) anchor: TextAnchor,
    /// An optional `transform="rotate(angle cx cy)"`.
    pub(crate) rotate: Option<(f64, f64, f64)>,
    pub(crate) font_family: &'a str,
    pub(crate) font_size: f64,
    /// Font weight; `None` emits no `font-weight` attribute (default `normal`).
    /// Spec §20.8.
    pub(crate) font_weight: Option<FontWeightIr>,
    /// Font style; emits `font-style="italic"` only when italic. Spec §20.8.
    pub(crate) font_style: FontStyleIr,
    pub(crate) fill: &'a str,
    pub(crate) opacity: Option<f64>,
    /// Logical text content. A `\n` splits into stacked tspans in SVG.
    pub(crate) content: &'a str,
}

impl TextRun<'_> {
    /// The default typography (`normal` weight, upright) used by data-mark text
    /// runs that are not styled by a theme text token.
    pub(crate) const DEFAULT_WEIGHT: Option<FontWeightIr> = None;
    pub(crate) const DEFAULT_STYLE: FontStyleIr = FontStyleIr::Normal;
}

/// A backend-neutral primitive sink (spec §24.6).
///
/// Implementors serialize each primitive into one output format. The methods map
/// directly to the SVG elements the renderer emits; `SvgSink` reproduces those
/// elements byte for byte and `DrawListSink` records an equivalent op.
pub(crate) trait MarkSink {
    /// Open a layer group (an SVG `<g class="...">`). The class names the chart
    /// region so the draw list can assign a role.
    fn open_layer(&mut self, class: &str);
    /// Close the most recently opened layer.
    fn close_layer(&mut self);

    /// Attach interaction metadata to every shape primitive emitted until the
    /// matching [`MarkSink::end_mark`] (spec §14.25, §24.6). An empty mark is a
    /// no-op, so geometries can call this unconditionally per datum. Nesting is
    /// not supported: each `begin_mark` MUST be paired with an `end_mark`.
    fn begin_mark(&mut self, mark: MarkInteraction);
    /// Clear the interaction metadata set by [`MarkSink::begin_mark`].
    fn end_mark(&mut self);

    /// Open a rectangular clip scope for data marks. Cartesian panels use this
    /// to hide out-of-view primitives after scales and stats have been trained.
    fn open_clip(&mut self, rect: Rect);
    /// Open a circular clip scope, used by circular inset plots.
    fn open_circle_clip(&mut self, cx: f64, cy: f64, r: f64);
    /// Close the most recently opened clip scope.
    fn close_clip(&mut self);

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, paint: &Paint);
    fn circle(&mut self, cx: f64, cy: f64, r: f64, paint: &Paint);
    fn path(&mut self, d: &str, paint: &Paint);
    fn path_with_dash(&mut self, d: &str, paint: &Paint, dash: Option<Dash>);
    fn polygon(&mut self, points: &str, paint: &Paint);
    fn image(&mut self, href: &str, x: f64, y: f64, width: f64, height: f64, opacity: Option<f64>);
    #[allow(clippy::too_many_arguments)]
    fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        width: f64,
        linecap_round: bool,
        opacity: Option<f64>,
        dash: Option<Dash>,
    );
    fn text(&mut self, run: &TextRun<'_>);

    // --- Byte-exact specials (geo/graticule) -------------------------------

    /// A `Geo` point marker: `<circle ... fill="C"[ opacity]/>` (compact close).
    fn geo_point(&mut self, cx: f64, cy: f64, r: f64, fill: &str, opacity: Option<f64>);
    /// A `Geo` path with even-odd fill for areal features (compact close).
    #[allow(clippy::too_many_arguments)]
    fn geo_path(
        &mut self,
        d: &str,
        fill: Option<&str>,
        stroke: Option<&str>,
        stroke_width: f64,
        opacity: Option<f64>,
        areal: bool,
    );
    /// A `Graticule` path: `<path ... fill="none" stroke=... [opacity]/>`.
    fn graticule_path(&mut self, d: &str, stroke: &str, width: f64, opacity: Option<f64>);

    /// The number of primitives emitted so far, for the "produced no marks"
    /// check (spec §26.3). Layer open/close does not count.
    fn primitive_count(&self) -> usize;
}

/// Escape a string as a JSON string literal (including surrounding quotes).
pub(crate) fn json_string(value: &str) -> String {
    use std::fmt::Write as _;
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

// --- SvgSink ----------------------------------------------------------------

/// A [`MarkSink`] that writes canonical SVG into an [`SvgWriter`] (spec §18).
pub(crate) struct SvgSink<'w> {
    w: &'w mut SvgWriter,
    count: usize,
    clip_id: usize,
    /// Interaction metadata for the mark currently being emitted (spec §14.25).
    /// `None` (the common case) produces byte-for-byte unchanged SVG.
    mark: Option<MarkInteraction>,
}

impl<'w> SvgSink<'w> {
    pub(crate) fn new(w: &'w mut SvgWriter) -> Self {
        SvgSink {
            w,
            count: 0,
            clip_id: 0,
            mark: None,
        }
    }

    /// Close a shape element `s` (which holds the open tag and all attributes,
    /// without the terminating `" />"`). With no active mark this writes the
    /// canonical self-closing form, byte-for-byte unchanged. With an active mark
    /// it appends a stable `data-algraf-highlight` attribute and/or wraps an
    /// accessible `<title>` child (spec §14.25, §18.10).
    fn finish_shape(&mut self, mut s: String, tag: &str) {
        let mark = match &self.mark {
            Some(mark) if !mark.is_empty() => mark,
            _ => {
                s.push_str(" />");
                self.w.line(&s);
                return;
            }
        };
        if let Some(group) = &mark.highlight {
            s.push_str(" data-algraf-highlight=\"");
            s.push_str(&escape_attr(group));
            s.push('"');
        }
        if let Some(event) = &mark.event {
            s.push_str(" data-algraf-event=\"");
            s.push_str(&escape_attr(&event.event));
            s.push_str("\" data-algraf-emit-field=\"");
            s.push_str(&escape_attr(&event.emit_field));
            s.push('"');
            if let Some(value) = &event.value {
                s.push_str(" data-algraf-emit-value=\"");
                s.push_str(&escape_attr(value));
                s.push('"');
            }
        }
        match &mark.tooltip {
            Some(tooltip) => {
                s.push_str("><title>");
                s.push_str(&escape_text(tooltip));
                s.push_str("</title></");
                s.push_str(tag);
                s.push('>');
            }
            None => s.push_str(" />"),
        }
        self.w.line(&s);
    }
}

impl MarkSink for SvgSink<'_> {
    fn open_layer(&mut self, class: &str) {
        self.w
            .open_group(&format!("class=\"{}\"", escape_attr(class)));
    }

    fn close_layer(&mut self) {
        self.w.close_group();
    }

    fn begin_mark(&mut self, mark: MarkInteraction) {
        self.mark = (!mark.is_empty()).then_some(mark);
    }

    fn end_mark(&mut self) {
        self.mark = None;
    }

    fn open_clip(&mut self, rect: Rect) {
        let id = self.clip_id;
        self.clip_id += 1;
        self.w.line("<defs>");
        self.w.line(&format!(
            "<clipPath id=\"algraf-clip-{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" /></clipPath>",
            num(rect.x),
            num(rect.y),
            num(rect.width),
            num(rect.height),
        ));
        self.w.line("</defs>");
        self.w
            .open_group(&format!("clip-path=\"url(#algraf-clip-{id})\""));
    }

    fn open_circle_clip(&mut self, cx: f64, cy: f64, r: f64) {
        let id = self.clip_id;
        self.clip_id += 1;
        self.w.line("<defs>");
        self.w.line(&format!(
            "<clipPath id=\"algraf-clip-{id}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\" /></clipPath>",
            num(cx),
            num(cy),
            num(r.max(0.0)),
        ));
        self.w.line("</defs>");
        self.w
            .open_group(&format!("clip-path=\"url(#algraf-clip-{id})\""));
    }

    fn close_clip(&mut self) {
        self.w.close_group();
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, paint: &Paint) {
        self.count += 1;
        let mut s = format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            num(x),
            num(y),
            num(width),
            num(height),
        );
        paint.write_svg(&mut s);
        self.finish_shape(s, "rect");
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, paint: &Paint) {
        self.count += 1;
        let mut s = format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"",
            num(cx),
            num(cy),
            num(r)
        );
        paint.write_svg(&mut s);
        self.finish_shape(s, "circle");
    }

    fn path(&mut self, d: &str, paint: &Paint) {
        self.path_with_dash(d, paint, None);
    }

    fn path_with_dash(&mut self, d: &str, paint: &Paint, dash: Option<Dash>) {
        self.count += 1;
        let mut s = format!("<path d=\"{d}\"");
        paint.write_svg(&mut s);
        if let Some(dash) = dash {
            s.push_str(&format!(" stroke-dasharray=\"{}\"", dash.dasharray()));
        }
        self.finish_shape(s, "path");
    }

    fn polygon(&mut self, points: &str, paint: &Paint) {
        self.count += 1;
        let mut s = format!("<polygon points=\"{points}\"");
        paint.write_svg(&mut s);
        self.finish_shape(s, "polygon");
    }

    fn image(&mut self, href: &str, x: f64, y: f64, width: f64, height: f64, opacity: Option<f64>) {
        self.count += 1;
        let mut s = format!(
            "<image href=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            escape_attr(href),
            num(x),
            num(y),
            num(width.max(0.0)),
            num(height.max(0.0)),
        );
        if let Some(opacity) = opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        self.finish_shape(s, "image");
    }

    fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        width: f64,
        linecap_round: bool,
        opacity: Option<f64>,
        dash: Option<Dash>,
    ) {
        self.count += 1;
        let mut s = format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"",
            num(x1),
            num(y1),
            num(x2),
            num(y2),
            escape_attr(stroke),
            num(width.max(0.0)),
        );
        if linecap_round {
            s.push_str(" stroke-linecap=\"round\"");
        }
        if let Some(opacity) = opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        if let Some(dash) = dash {
            s.push_str(&format!(" stroke-dasharray=\"{}\"", dash.dasharray()));
        }
        s.push_str(" />");
        self.w.line(&s);
    }

    fn text(&mut self, run: &TextRun<'_>) {
        self.count += 1;
        let mut s = format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\"",
            num(run.x),
            num(run.y),
            run.anchor.as_str(),
        );
        if let Some((angle, cx, cy)) = run.rotate {
            s.push_str(&format!(
                " transform=\"rotate({} {} {})\"",
                num(angle),
                num(cx),
                num(cy)
            ));
        }
        s.push_str(&format!(
            " font-family=\"{}\" font-size=\"{}\"",
            escape_attr(run.font_family),
            num(run.font_size),
        ));
        if let Some(weight) = run.font_weight.and_then(FontWeightIr::svg_attr) {
            s.push_str(&format!(" font-weight=\"{}\"", escape_attr(&weight)));
        }
        if let Some(style) = run.font_style.svg_attr() {
            s.push_str(&format!(" font-style=\"{style}\""));
        }
        s.push_str(&format!(" fill=\"{}\"", escape_attr(run.fill)));
        if let Some(opacity) = run.opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        s.push('>');
        if run.content.contains('\n') {
            for (i, line) in run.content.split('\n').enumerate() {
                let line = line.strip_suffix('\r').unwrap_or(line);
                let dy = if i == 0 { "0" } else { "1.2em" };
                s.push_str(&format!(
                    "<tspan x=\"{}\" dy=\"{}\">{}</tspan>",
                    num(run.x),
                    dy,
                    escape_text(line),
                ));
            }
        } else {
            s.push_str(&escape_text(run.content));
        }
        s.push_str("</text>");
        self.w.line(&s);
    }

    fn geo_point(&mut self, cx: f64, cy: f64, r: f64, fill: &str, opacity: Option<f64>) {
        self.count += 1;
        let mut s = format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"",
            num(cx),
            num(cy),
            num(r),
            escape_attr(fill),
        );
        if let Some(opacity) = opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        s.push_str("/>");
        self.w.line(&s);
    }

    fn geo_path(
        &mut self,
        d: &str,
        fill: Option<&str>,
        stroke: Option<&str>,
        stroke_width: f64,
        opacity: Option<f64>,
        areal: bool,
    ) {
        self.count += 1;
        let mut s = format!("<path d=\"{d}\"");
        if areal {
            s.push_str(&format!(
                " fill=\"{}\"",
                escape_attr(fill.unwrap_or("#4E79A7"))
            ));
            s.push_str(" fill-rule=\"evenodd\"");
        } else {
            s.push_str(" fill=\"none\"");
        }
        if let Some(color) = stroke {
            s.push_str(&format!(
                " stroke=\"{}\" stroke-width=\"{}\"",
                escape_attr(color),
                num(stroke_width.max(0.0)),
            ));
        }
        if let Some(opacity) = opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        s.push_str("/>");
        self.w.line(&s);
    }

    fn graticule_path(&mut self, d: &str, stroke: &str, width: f64, opacity: Option<f64>) {
        self.count += 1;
        let mut s = format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"",
            d,
            escape_attr(stroke),
            num(width.max(0.0)),
        );
        if let Some(opacity) = opacity {
            s.push_str(&format!(" opacity=\"{}\"", num(opacity)));
        }
        s.push_str("/>");
        self.w.line(&s);
    }

    fn primitive_count(&self) -> usize {
        self.count
    }
}

// --- DrawListSink -----------------------------------------------------------

/// A [`MarkSink`] that records primitives as [`DrawOp`]s (spec §24.6). The role
/// of each op is derived from the enclosing layer class.
pub(crate) struct DrawListSink {
    ops: Vec<DrawOp>,
    role: DrawRole,
    /// Interaction metadata for the mark currently being emitted (spec §14.25).
    mark: Option<MarkInteraction>,
}

impl DrawListSink {
    pub(crate) fn new() -> Self {
        DrawListSink {
            ops: Vec::new(),
            role: DrawRole::Mark,
            mark: None,
        }
    }

    /// The interaction metadata to record on the next shape op, if any.
    fn current_mark(&self) -> Option<MarkInteraction> {
        self.mark.clone().filter(|m| !m.is_empty())
    }

    pub(crate) fn into_ops(self) -> Vec<DrawOp> {
        self.ops
    }
}

/// Map a layer class to the draw-list role its primitives carry.
fn role_for_layer(class: &str) -> DrawRole {
    if class.starts_with("algraf-layer") {
        DrawRole::Mark
    } else if class == "algraf-grid" || class == "algraf-polar-grid" {
        DrawRole::Grid
    } else if class == "algraf-axes" {
        DrawRole::Axis
    } else if class == "algraf-legends" {
        DrawRole::Legend
    } else if class.starts_with("algraf-polar") {
        DrawRole::PolarLabel
    } else if class == "algraf-facet-strip" {
        DrawRole::FacetStrip
    } else {
        DrawRole::Mark
    }
}

impl MarkSink for DrawListSink {
    fn open_layer(&mut self, class: &str) {
        self.role = role_for_layer(class);
    }

    fn close_layer(&mut self) {
        self.role = DrawRole::Mark;
    }

    fn begin_mark(&mut self, mark: MarkInteraction) {
        self.mark = (!mark.is_empty()).then_some(mark);
    }

    fn end_mark(&mut self) {
        self.mark = None;
    }

    fn open_clip(&mut self, rect: Rect) {
        self.ops.push(DrawOp::ClipStart {
            role: self.role,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
    }

    fn open_circle_clip(&mut self, cx: f64, cy: f64, r: f64) {
        self.ops.push(DrawOp::CircleClipStart {
            role: self.role,
            cx,
            cy,
            r: r.max(0.0),
        });
    }

    fn close_clip(&mut self) {
        self.ops.push(DrawOp::ClipEnd { role: self.role });
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, paint: &Paint) {
        let interaction = self.current_mark();
        self.ops.push(DrawOp::Rect {
            role: self.role,
            x,
            y,
            width,
            height,
            paint: paint.clone(),
            interaction,
        });
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, paint: &Paint) {
        let interaction = self.current_mark();
        self.ops.push(DrawOp::Circle {
            role: self.role,
            cx,
            cy,
            r,
            paint: paint.clone(),
            interaction,
        });
    }

    fn path(&mut self, d: &str, paint: &Paint) {
        self.path_with_dash(d, paint, None);
    }

    fn path_with_dash(&mut self, d: &str, paint: &Paint, dash: Option<Dash>) {
        let interaction = self.current_mark();
        self.ops.push(DrawOp::Path {
            role: self.role,
            d: d.to_string(),
            paint: paint.clone(),
            dash,
            interaction,
        });
    }

    fn polygon(&mut self, points: &str, paint: &Paint) {
        let interaction = self.current_mark();
        self.ops.push(DrawOp::Polygon {
            role: self.role,
            points: points.to_string(),
            paint: paint.clone(),
            interaction,
        });
    }

    fn image(&mut self, href: &str, x: f64, y: f64, width: f64, height: f64, opacity: Option<f64>) {
        let interaction = self.current_mark();
        self.ops.push(DrawOp::Image {
            role: self.role,
            href: href.to_string(),
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
            opacity,
            interaction,
        });
    }

    fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        width: f64,
        linecap_round: bool,
        opacity: Option<f64>,
        dash: Option<Dash>,
    ) {
        self.ops.push(DrawOp::Line {
            role: self.role,
            x1,
            y1,
            x2,
            y2,
            stroke: stroke.to_string(),
            stroke_width: width.max(0.0),
            linecap_round,
            opacity,
            dash,
        });
    }

    fn text(&mut self, run: &TextRun<'_>) {
        self.ops.push(DrawOp::Text {
            role: self.role,
            x: run.x,
            y: run.y,
            anchor: run.anchor,
            fill: run.fill.to_string(),
            opacity: run.opacity,
            content: run.content.to_string(),
        });
    }

    fn geo_point(&mut self, cx: f64, cy: f64, r: f64, fill: &str, opacity: Option<f64>) {
        self.ops.push(DrawOp::Circle {
            role: self.role,
            cx,
            cy,
            r,
            paint: Paint {
                fill: Fill::Color(fill.to_string()),
                stroke: Stroke::Omit,
                opacity,
            },
            interaction: None,
        });
    }

    fn geo_path(
        &mut self,
        d: &str,
        fill: Option<&str>,
        stroke: Option<&str>,
        stroke_width: f64,
        opacity: Option<f64>,
        areal: bool,
    ) {
        let fill = if areal {
            Fill::Color(fill.unwrap_or("#4E79A7").to_string())
        } else {
            Fill::None
        };
        let stroke = match stroke {
            Some(color) => Stroke::Solid {
                color: color.to_string(),
                width: stroke_width.max(0.0),
            },
            None => Stroke::Omit,
        };
        self.ops.push(DrawOp::Path {
            role: self.role,
            d: d.to_string(),
            paint: Paint {
                fill,
                stroke,
                opacity,
            },
            dash: None,
            interaction: None,
        });
    }

    fn graticule_path(&mut self, d: &str, stroke: &str, width: f64, opacity: Option<f64>) {
        self.ops.push(DrawOp::Path {
            role: self.role,
            d: d.to_string(),
            paint: Paint {
                fill: Fill::None,
                stroke: Stroke::Solid {
                    color: stroke.to_string(),
                    width: width.max(0.0),
                },
                opacity,
            },
            dash: None,
            interaction: None,
        });
    }

    fn primitive_count(&self) -> usize {
        self.ops.len()
    }
}
