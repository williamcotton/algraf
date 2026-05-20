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

/// One facet panel's guide strip and plot rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacetPanel {
    pub strip: Rect,
    pub plot: Rect,
}

/// Computed layout rectangles (spec §17.2).
#[derive(Debug, Clone)]
pub struct Layout {
    pub svg: Rect,
    pub plot: Rect,
    pub legend: Option<Rect>,
    pub facets: Vec<FacetPanel>,
}

const MARGIN_TOP: f64 = 40.0;
const MARGIN_RIGHT: f64 = 30.0;
const MARGIN_BOTTOM: f64 = 50.0;
const MARGIN_LEFT: f64 = 60.0;
const LEGEND_WIDTH: f64 = 120.0;
const FACET_GAP_X: f64 = 24.0;
const FACET_AXIS_GAP_X: f64 = 72.0;
const FACET_GAP_Y: f64 = 28.0;
const FACET_AXIS_GAP_Y: f64 = 64.0;
const FACET_STRIP_HEIGHT: f64 = 18.0;
const FACET_STRIP_GAP: f64 = 6.0;

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
            facets: Vec::new(),
        }
    }

    /// Compute a facet-wrap layout inside the ordinary plot area (spec §17.4).
    pub fn compute_facets(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        panel_count: usize,
        columns: Option<usize>,
    ) -> Layout {
        let mut layout = Layout::compute(width, height, has_legend, has_axes);
        let panel_count = panel_count.max(1);
        let columns = columns
            .filter(|c| *c > 0)
            .unwrap_or_else(|| default_facet_columns(panel_count, layout.plot))
            .clamp(1, panel_count);
        let rows = panel_count.div_ceil(columns);

        let gap_x = if has_axes {
            FACET_AXIS_GAP_X
        } else {
            FACET_GAP_X
        };
        let gap_y = if has_axes {
            FACET_AXIS_GAP_Y
        } else {
            FACET_GAP_Y
        };
        let total_gap_x = gap_x * columns.saturating_sub(1) as f64;
        let total_gap_y = gap_y * rows.saturating_sub(1) as f64;
        let cell_width = ((layout.plot.width - total_gap_x) / columns as f64).max(1.0);
        let cell_height = ((layout.plot.height - total_gap_y) / rows as f64).max(1.0);

        let strip_height = FACET_STRIP_HEIGHT.min((cell_height * 0.25).max(0.0));
        let strip_gap = if strip_height > 0.0 {
            FACET_STRIP_GAP.min(cell_height * 0.1)
        } else {
            0.0
        };
        let plot_height = (cell_height - strip_height - strip_gap).max(1.0);

        layout.facets = (0..panel_count)
            .map(|index| {
                let col = index % columns;
                let row = index / columns;
                let x = layout.plot.x + col as f64 * (cell_width + gap_x);
                let y = layout.plot.y + row as f64 * (cell_height + gap_y);
                let strip = Rect {
                    x,
                    y,
                    width: cell_width,
                    height: strip_height,
                };
                let plot = Rect {
                    x,
                    y: y + strip_height + strip_gap,
                    width: cell_width,
                    height: plot_height,
                };
                FacetPanel { strip, plot }
            })
            .collect();

        layout
    }
}

fn default_facet_columns(panel_count: usize, plot: Rect) -> usize {
    let aspect = (plot.width / plot.height.max(1.0)).clamp(0.25, 4.0);
    ((panel_count as f64 * aspect).sqrt().round() as usize)
        .max(1)
        .min(panel_count.max(1))
}
