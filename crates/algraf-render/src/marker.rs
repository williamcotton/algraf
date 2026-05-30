//! Marker shapes shared between the `Point` geometry and shape legends (spec
//! §16.10, §19.5). Categorical shape mappings assign shapes deterministically in
//! domain order and wrap when there are more categories than supported shapes,
//! so the legend draws the same glyph the plot does for each category.

use crate::sink::{MarkSink, Paint};
use crate::svg::num;

/// A point marker shape. Version 0.3.0 point shapes are circle, square,
/// triangle, and diamond (spec §16.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerShape {
    Circle,
    Square,
    Triangle,
    Diamond,
}

/// The supported shapes in the order categorical mappings assign them.
pub(crate) const MARKER_SHAPES: &[MarkerShape] = &[
    MarkerShape::Circle,
    MarkerShape::Square,
    MarkerShape::Triangle,
    MarkerShape::Diamond,
];

/// The shape assigned to the category at `index` in domain order, wrapping when
/// there are more categories than supported shapes (spec §16.10).
pub(crate) fn marker_for_index(index: usize) -> MarkerShape {
    MARKER_SHAPES[index % MARKER_SHAPES.len()]
}

/// Parse a literal shape name, returning `None` for an unknown name so the
/// caller can warn and fall back to `circle`.
pub(crate) fn parse_marker_shape(name: &str) -> Option<MarkerShape> {
    match name {
        "circle" => Some(MarkerShape::Circle),
        "square" => Some(MarkerShape::Square),
        "triangle" => Some(MarkerShape::Triangle),
        "diamond" => Some(MarkerShape::Diamond),
        _ => None,
    }
}

/// Draw a marker centered at `(cx, cy)` with half-extent `size` (the circle
/// radius; other shapes share the same bounding half-extent). Both the plot and
/// the legend route through this so a category's swatch matches its marks.
pub(crate) fn emit_marker(
    sink: &mut dyn MarkSink,
    shape: MarkerShape,
    cx: f64,
    cy: f64,
    size: f64,
    paint: &Paint,
) {
    match shape {
        MarkerShape::Circle => sink.circle(cx, cy, size, paint),
        MarkerShape::Square => {
            let side = size * 2.0;
            sink.rect(cx - size, cy - size, side, side, paint);
        }
        MarkerShape::Triangle => {
            let d = format!(
                "M{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy + size),
                num(cx - size),
                num(cy + size)
            );
            sink.path(&d, paint);
        }
        MarkerShape::Diamond => {
            let d = format!(
                "M{} {} L{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy),
                num(cx),
                num(cy + size),
                num(cx - size),
                num(cy)
            );
            sink.path(&d, paint);
        }
    }
}
