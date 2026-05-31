//! The render-model raster backend (spec §24.6).
//!
//! This is the third [`RenderBackend`]. Unlike the CLI's PNG wrapper, which
//! rasterizes the *SVG bytes*, this backend draws directly from the planned
//! scene's [`DrawList`] using `tiny-skia` — no SVG parser, no browser runtime,
//! no system fonts. It proves the draw list is a complete, self-sufficient scene
//! description.
//!
//! # Documented equivalence limits (spec §24.6, Design Decision 4)
//!
//! - **Text glyphs are not rendered.** Text positions, anchors, and content are
//!   present in the draw list, but turning them into glyph outlines needs a font
//!   shaper, which would pull in fonts and defeat determinism. The canonical,
//!   pixel-faithful raster path remains the SVG-rasterizing PNG wrapper (§22.3);
//!   this backend renders the chart's shape primitives. The SVG backend defines
//!   the intended appearance.
//! - **Anti-aliasing is platform-dependent.** `tiny-skia`'s AA is deterministic
//!   for a given platform/version but may differ across them; golden comparisons
//!   use a tolerance.
//! - **`fill-rule` is winding.** Even-odd fills (geographic polygons with holes)
//!   are approximated.

use algraf_core::{codes, Diagnostic};
use tiny_skia::{
    Color, FillRule, LineCap, Mask, Paint as SkPaint, PathBuilder, Pixmap, Stroke as SkStroke,
    StrokeDash, Transform,
};

use crate::sink::{Dash, Fill, Stroke};

use super::backend::{RenderBackend, RenderScene};
use super::draw_list::{DrawListBackend, DrawOp};
use super::metadata::InteractionMetadata;

/// A rasterized chart image.
pub struct RasterImage {
    pixmap: Pixmap,
}

impl RasterImage {
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Premultiplied RGBA8 pixel data, row-major.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Encode the image as PNG bytes.
    pub fn encode_png(&self) -> Result<Vec<u8>, png::EncodingError> {
        self.pixmap.encode_png()
    }

    /// The backing pixmap, for callers that add their own PNG metadata (e.g. DPI).
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }
}

impl std::fmt::Debug for RasterImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RasterImage")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

/// The render-model raster backend. `scale` multiplies the SVG viewport to the
/// raster pixel grid (matching the CLI's `--png-scale`).
pub(super) struct RasterBackend {
    pub(super) scale: f32,
}

impl RenderBackend for RasterBackend {
    type Output = RasterImage;

    fn emit(
        &self,
        scene: &RenderScene<'_>,
        metadata: &InteractionMetadata,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> RasterImage {
        // Draw from the same complete draw list the draw-list backend produces.
        let list = DrawListBackend.emit(scene, metadata, diagnostics);
        rasterize(&list, self.scale, diagnostics)
    }
}

/// Rasterize a complete [`DrawList`](super::draw_list::DrawList) at `scale`.
fn rasterize(
    list: &super::draw_list::DrawList,
    scale: f32,
    diagnostics: &mut Vec<Diagnostic>,
) -> RasterImage {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = ((list.width as f32) * scale).round().max(1.0) as u32;
    let height = ((list.height as f32) * scale).round().max(1.0) as u32;
    let mut pixmap = Pixmap::new(width, height).unwrap_or_else(|| Pixmap::new(1, 1).expect("1x1"));
    let transform = Transform::from_scale(scale, scale);

    let mut clip_stack: Vec<Mask> = Vec::new();
    for op in &list.ops {
        match op {
            DrawOp::ClipStart {
                x,
                y,
                width: clip_width,
                height: clip_height,
                ..
            } => {
                if let Some(mask) =
                    clip_mask(width, height, transform, *x, *y, *clip_width, *clip_height)
                {
                    clip_stack.push(mask);
                }
            }
            DrawOp::ClipEnd { .. } => {
                clip_stack.pop();
            }
            _ => draw_op(&mut pixmap, transform, op, clip_stack.last(), diagnostics),
        }
    }

    RasterImage { pixmap }
}

fn clip_mask(
    width: u32,
    height: u32,
    transform: Transform,
    x: f64,
    y: f64,
    rect_width: f64,
    rect_height: f64,
) -> Option<Mask> {
    let mut mask = Mask::new(width, height)?;
    let rect = tiny_skia::Rect::from_xywh(
        x as f32,
        y as f32,
        rect_width.max(0.0) as f32,
        rect_height.max(0.0) as f32,
    )?;
    let path = PathBuilder::from_rect(rect);
    mask.fill_path(&path, FillRule::Winding, true, transform);
    Some(mask)
}

fn draw_op(
    pixmap: &mut Pixmap,
    transform: Transform,
    op: &DrawOp,
    mask: Option<&Mask>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match op {
        DrawOp::ClipStart { .. } | DrawOp::ClipEnd { .. } => {}
        DrawOp::Rect {
            x,
            y,
            width,
            height,
            paint,
            ..
        } => {
            let Some(rect) =
                tiny_skia::Rect::from_xywh(*x as f32, *y as f32, *width as f32, *height as f32)
            else {
                return;
            };
            let path = PathBuilder::from_rect(rect);
            fill_and_stroke(pixmap, transform, &path, paint, None, mask);
        }
        DrawOp::Circle {
            cx, cy, r, paint, ..
        } => {
            let mut pb = PathBuilder::new();
            pb.push_circle(*cx as f32, *cy as f32, *r as f32);
            if let Some(path) = pb.finish() {
                fill_and_stroke(pixmap, transform, &path, paint, None, mask);
            }
        }
        DrawOp::Polygon { points, paint, .. } => {
            if let Some(path) = polygon_path(points) {
                fill_and_stroke(pixmap, transform, &path, paint, None, mask);
            } else {
                unrepresentable(diagnostics, "polygon points");
            }
        }
        DrawOp::Path { d, paint, dash, .. } => match path_from_d(d) {
            Some(path) => fill_and_stroke(pixmap, transform, &path, paint, *dash, mask),
            None => unrepresentable(diagnostics, "path data"),
        },
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
            let mut pb = PathBuilder::new();
            pb.move_to(*x1 as f32, *y1 as f32);
            pb.line_to(*x2 as f32, *y2 as f32);
            if let Some(path) = pb.finish() {
                let mut sk = SkPaint {
                    anti_alias: true,
                    ..SkPaint::default()
                };
                sk.set_color(parse_color(stroke, opacity.unwrap_or(1.0)));
                let mut s = SkStroke {
                    width: (*stroke_width as f32).max(0.0),
                    ..SkStroke::default()
                };
                if *linecap_round {
                    s.line_cap = LineCap::Round;
                }
                if let Some(dash) = dash {
                    s.dash = stroke_dash(*dash, *stroke_width as f32);
                }
                pixmap.stroke_path(&path, &sk, &s, transform, mask);
            }
        }
        // Text glyphs are a documented equivalence limit (see module docs).
        DrawOp::Text { .. } => {}
    }
}

/// Fill (and, when present, stroke) a shape path with a draw-list paint.
fn fill_and_stroke(
    pixmap: &mut Pixmap,
    transform: Transform,
    path: &tiny_skia::Path,
    paint: &crate::sink::Paint,
    dash: Option<Dash>,
    mask: Option<&Mask>,
) {
    let opacity = paint.opacity.unwrap_or(1.0);
    if let Fill::Color(color) = &paint.fill {
        let mut sk = SkPaint {
            anti_alias: true,
            ..SkPaint::default()
        };
        sk.set_color(parse_color(color, opacity));
        pixmap.fill_path(path, &sk, FillRule::Winding, transform, mask);
    }
    if let Stroke::Solid { color, width } = &paint.stroke {
        let mut sk = SkPaint {
            anti_alias: true,
            ..SkPaint::default()
        };
        sk.set_color(parse_color(color, opacity));
        let mut s = SkStroke {
            width: (*width as f32).max(0.0),
            ..SkStroke::default()
        };
        if let Some(dash) = dash {
            s.dash = stroke_dash(dash, *width as f32);
        }
        pixmap.stroke_path(path, &sk, &s, transform, mask);
    }
}

fn stroke_dash(dash: Dash, width: f32) -> Option<StrokeDash> {
    let array = match dash {
        // Match the SVG presets, scaled to the stroke width like SVG dashes.
        Dash::Dotted => vec![width, 2.0 * width],
        Dash::Dashed => vec![4.0 * width, 4.0 * width],
    };
    StrokeDash::new(array, 0.0)
}

/// Build a path from a space-separated `x,y` polygon point list.
fn polygon_path(points: &str) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let mut started = false;
    for pair in points.split_whitespace() {
        let (xs, ys) = pair.split_once(',')?;
        let x: f32 = xs.parse().ok()?;
        let y: f32 = ys.parse().ok()?;
        if started {
            pb.line_to(x, y);
        } else {
            pb.move_to(x, y);
            started = true;
        }
    }
    if !started {
        return None;
    }
    pb.close();
    pb.finish()
}

/// Parse an SVG path `d` string (the `M`/`L`/`A`/`Z` absolute subset the renderer
/// emits) into a `tiny-skia` path. Arcs are sampled into line segments.
fn path_from_d(d: &str) -> Option<tiny_skia::Path> {
    // Separate command letters from numbers so "M10 20" tokenizes cleanly.
    let mut spaced = String::with_capacity(d.len() * 2);
    for c in d.chars() {
        if c.is_ascii_alphabetic() {
            spaced.push(' ');
            spaced.push(c);
            spaced.push(' ');
        } else {
            spaced.push(c);
        }
    }
    let toks: Vec<&str> = spaced.split([' ', ',']).filter(|s| !s.is_empty()).collect();

    let mut pb = PathBuilder::new();
    let mut i = 0;
    let mut cur = (0.0_f32, 0.0_f32);
    let num = |toks: &[&str], i: &mut usize| -> Option<f32> {
        let v = toks.get(*i)?.parse::<f32>().ok()?;
        *i += 1;
        Some(v)
    };
    while i < toks.len() {
        let cmd = toks[i];
        i += 1;
        match cmd {
            "M" => {
                let x = num(&toks, &mut i)?;
                let y = num(&toks, &mut i)?;
                pb.move_to(x, y);
                cur = (x, y);
            }
            "L" => {
                let x = num(&toks, &mut i)?;
                let y = num(&toks, &mut i)?;
                pb.line_to(x, y);
                cur = (x, y);
            }
            "A" => {
                let rx = num(&toks, &mut i)?;
                let ry = num(&toks, &mut i)?;
                let phi = num(&toks, &mut i)?;
                let large = num(&toks, &mut i)? != 0.0;
                let sweep = num(&toks, &mut i)? != 0.0;
                let x = num(&toks, &mut i)?;
                let y = num(&toks, &mut i)?;
                append_arc(&mut pb, cur, rx, ry, phi, large, sweep, (x, y));
                cur = (x, y);
            }
            "Z" | "z" => pb.close(),
            _ => return None,
        }
    }
    pb.finish()
}

/// Append an SVG elliptic-arc segment to the path, sampled as line segments
/// (endpoint-to-center per SVG implementation notes F.6.5/F.6.6).
#[allow(clippy::too_many_arguments)]
fn append_arc(
    pb: &mut PathBuilder,
    start: (f32, f32),
    rx: f32,
    ry: f32,
    phi_deg: f32,
    large: bool,
    sweep: bool,
    end: (f32, f32),
) {
    let (x1, y1) = (start.0 as f64, start.1 as f64);
    let (x2, y2) = (end.0 as f64, end.1 as f64);
    let (mut rx, mut ry) = (rx as f64, ry as f64);
    if rx == 0.0 || ry == 0.0 {
        pb.line_to(end.0, end.1);
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = (phi_deg as f64).to_radians();
    let (cos_p, sin_p) = (phi.cos(), phi.sin());

    let dx2 = (x1 - x2) / 2.0;
    let dy2 = (y1 - y2) / 2.0;
    let x1p = cos_p * dx2 + sin_p * dy2;
    let y1p = -sin_p * dx2 + cos_p * dy2;

    // Correct out-of-range radii.
    let lambda = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let sign = if large != sweep { 1.0 } else { -1.0 };
    let num = (rx * rx * ry * ry) - (rx * rx * y1p * y1p) - (ry * ry * x1p * x1p);
    let den = (rx * rx * y1p * y1p) + (ry * ry * x1p * x1p);
    let coef = sign * (num.max(0.0) / den).sqrt();
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);

    let cx = cos_p * cxp - sin_p * cyp + (x1 + x2) / 2.0;
    let cy = sin_p * cxp + cos_p * cyp + (y1 + y2) / 2.0;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }

    let steps = ((delta.abs() / (std::f64::consts::PI / 36.0)).ceil() as usize).max(2);
    for s in 1..=steps {
        let t = theta1 + delta * (s as f64 / steps as f64);
        let px = cx + rx * t.cos() * cos_p - ry * t.sin() * sin_p;
        let py = cy + rx * t.cos() * sin_p + ry * t.sin() * cos_p;
        pb.line_to(px as f32, py as f32);
    }
}

/// Parse a color string (hex or a common CSS name) at the given opacity.
fn parse_color(s: &str, opacity: f64) -> Color {
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    if let Some(hex) = s.strip_prefix('#') {
        if let Some((r, g, b)) = parse_hex(hex) {
            return Color::from_rgba8(r, g, b, a);
        }
    }
    let (r, g, b) = named_color(s).unwrap_or((0x88, 0x88, 0x88));
    Color::from_rgba8(r, g, b, a)
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    match hex.len() {
        3 => {
            let v = |c: u8| {
                let d = (c as char).to_digit(16)? as u8;
                Some(d * 17)
            };
            let b = hex.as_bytes();
            Some((v(b[0])?, v(b[1])?, v(b[2])?))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn named_color(name: &str) -> Option<(u8, u8, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "gray" | "grey" => (128, 128, 128),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "orange" => (255, 165, 0),
        "steelblue" => (70, 130, 180),
        "tomato" => (255, 99, 71),
        "transparent" => return None,
        _ => return None,
    })
}

fn unrepresentable(diagnostics: &mut Vec<Diagnostic>, what: &str) {
    diagnostics.push(Diagnostic::warning(
        codes::R0005,
        format!("raster backend could not represent {what}"),
        algraf_core::Span::new(0, 0),
    ));
}
