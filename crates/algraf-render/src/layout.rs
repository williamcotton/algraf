//! Viewport layout with fixed margins (spec §17).

/// A rectangle in SVG coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Computed layout rectangles (spec §17.2).
#[derive(Debug, Clone)]
pub struct Layout {
    pub svg: Rect,
    pub plot: Rect,
    pub legend: Option<Rect>,
}

const MARGIN_TOP: f64 = 40.0;
const MARGIN_RIGHT: f64 = 30.0;
const MARGIN_BOTTOM: f64 = 50.0;
const MARGIN_LEFT: f64 = 60.0;
const LEGEND_WIDTH: f64 = 120.0;

impl Layout {
    /// Compute layout for the given SVG dimensions (spec §17.3, fixed margins).
    pub fn compute(width: f64, height: f64, has_legend: bool, has_axes: bool) -> Layout {
        let (top, right, bottom, left) = if has_axes {
            (MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT)
        } else {
            (10.0, 10.0, 10.0, 10.0)
        };
        let legend_reserve = if has_legend { LEGEND_WIDTH } else { 0.0 };

        let plot = Rect {
            x: left,
            y: top,
            width: (width - left - right - legend_reserve).max(1.0),
            height: (height - top - bottom).max(1.0),
        };

        let legend = has_legend.then(|| Rect {
            x: plot.right() + 16.0,
            y: plot.y,
            width: LEGEND_WIDTH - 16.0,
            height: plot.height,
        });

        Layout {
            svg: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            plot,
            legend,
        }
    }
}
