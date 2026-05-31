//! Viewport layout with fixed margins (spec §17).

use algraf_semantics::PanelSpacingIr;

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

/// Per-side configured plot margins in pixels (spec §17.3). With axes a
/// `Some(n)` value is a floor (the computed margin widens to at least `n`); with
/// no axes it sets the side exactly, down to 0. `None` keeps the computed
/// default for that side.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Margins {
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
}

const MARGIN_TOP: f64 = 40.0;
const MARGIN_RIGHT: f64 = 30.0;
pub(crate) const MARGIN_BOTTOM: f64 = 50.0;
pub(crate) const MARGIN_LEFT: f64 = 60.0;
/// Base padding per side when the chart has no axes (e.g. the `void` theme).
/// Unlike the axis margins this is pure padding, so a configured `margin*`
/// value overrides it outright — down to 0 — rather than acting as a floor.
const NO_AXES_MARGIN: f64 = 10.0;
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
        Layout::compute_with_text(
            width,
            height,
            has_legend,
            has_axes,
            0.0,
            0.0,
            0.0,
            Margins::default(),
        )
    }

    /// Compute layout with extra title/caption reserve. `left_extra` widens the
    /// left margin to make room for wide y tick labels (spec §17.3). `margins`
    /// applies per-side user minimums on top of the computed margins.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_with_text(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        top_extra: f64,
        bottom_extra: f64,
        left_extra: f64,
        margins: Margins,
    ) -> Layout {
        let (base_top, base_right, base_bottom, base_left) = if has_axes {
            (MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT)
        } else {
            (
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
            )
        };
        // Content reserve for chart title/subtitle/caption and wide y tick
        // labels. This is a hard minimum that an explicit margin never clips.
        let top_extra = top_extra.max(0.0);
        let bottom_extra = bottom_extra.max(0.0);
        let left_extra = left_extra.max(0.0);
        // Computed default margins = base padding + content reserve.
        let computed_top = base_top + top_extra;
        let computed_bottom = base_bottom + bottom_extra;
        let computed_left = base_left + left_extra;
        let (top, right, bottom, left) = if has_axes {
            // With axes the base margin holds the axis line and tick labels, so
            // a configured value acts as a floor: it can widen a side but never
            // shrink below what the guides require (spec §17.3).
            (
                margins.top.map_or(computed_top, |m| computed_top.max(m)),
                margins.right.map_or(base_right, |m| base_right.max(m)),
                margins
                    .bottom
                    .map_or(computed_bottom, |m| computed_bottom.max(m)),
                margins.left.map_or(computed_left, |m| computed_left.max(m)),
            )
        } else {
            // With no axes the base margin is pure padding, so a configured
            // value sets the side exactly (down to 0) for full-bleed sparklines,
            // floored only by the content reserve so explicit chart text is
            // never clipped (spec §17.3).
            (
                margins.top.map_or(computed_top, |m| m.max(top_extra)),
                margins.right.unwrap_or(base_right),
                margins
                    .bottom
                    .map_or(computed_bottom, |m| m.max(bottom_extra)),
                margins.left.map_or(computed_left, |m| m.max(left_extra)),
            )
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
        Layout::compute_facets_with_text(
            width,
            height,
            has_legend,
            has_axes,
            panel_count,
            columns,
            0.0,
            0.0,
            0.0,
            Margins::default(),
            None,
        )
    }

    /// Compute a facet-wrap layout with extra title/caption reserve.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_facets_with_text(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        panel_count: usize,
        columns: Option<usize>,
        top_extra: f64,
        bottom_extra: f64,
        left_extra: f64,
        margins: Margins,
        panel_spacing: Option<PanelSpacingIr>,
    ) -> Layout {
        let mut layout = Layout::compute_with_text(
            width,
            height,
            has_legend,
            has_axes,
            top_extra,
            bottom_extra,
            left_extra,
            margins,
        );
        let panel_count = panel_count.max(1);
        let columns = columns
            .filter(|c| *c > 0)
            .unwrap_or_else(|| default_facet_columns(panel_count, layout.plot))
            .clamp(1, panel_count);
        let rows = panel_count.div_ceil(columns);
        layout.populate_facets(rows, columns, panel_count, has_axes, panel_spacing);

        layout
    }

    /// Compute an exact row-by-column facet-grid layout.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_facet_grid_with_text(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        rows: usize,
        columns: usize,
        top_extra: f64,
        bottom_extra: f64,
        left_extra: f64,
        margins: Margins,
        panel_spacing: Option<PanelSpacingIr>,
    ) -> Layout {
        let mut layout = Layout::compute_with_text(
            width,
            height,
            has_legend,
            has_axes,
            top_extra,
            bottom_extra,
            left_extra,
            margins,
        );
        let rows = rows.max(1);
        let columns = columns.max(1);
        layout.populate_facets(rows, columns, rows * columns, has_axes, panel_spacing);
        layout
    }

    fn populate_facets(
        &mut self,
        rows: usize,
        columns: usize,
        panel_count: usize,
        has_axes: bool,
        panel_spacing: Option<PanelSpacingIr>,
    ) {
        let gap_x = panel_spacing.map_or_else(
            || {
                if has_axes {
                    FACET_AXIS_GAP_X
                } else {
                    FACET_GAP_X
                }
            },
            |spacing| spacing.x,
        );
        let gap_y = panel_spacing.map_or_else(
            || {
                if has_axes {
                    FACET_AXIS_GAP_Y
                } else {
                    FACET_GAP_Y
                }
            },
            |spacing| spacing.y,
        );
        let total_gap_x = gap_x * columns.saturating_sub(1) as f64;
        let total_gap_y = gap_y * rows.saturating_sub(1) as f64;
        let cell_width = ((self.plot.width - total_gap_x) / columns as f64).max(1.0);
        let cell_height = ((self.plot.height - total_gap_y) / rows as f64).max(1.0);

        let strip_height = FACET_STRIP_HEIGHT.min((cell_height * 0.25).max(0.0));
        let strip_gap = if strip_height > 0.0 {
            FACET_STRIP_GAP.min(cell_height * 0.1)
        } else {
            0.0
        };
        let plot_height = (cell_height - strip_height - strip_gap).max(1.0);

        self.facets = (0..panel_count)
            .map(|index| {
                let col = index % columns;
                let row = index / columns;
                let x = self.plot.x + col as f64 * (cell_width + gap_x);
                let y = self.plot.y + row as f64 * (cell_height + gap_y);
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
    }
}

fn default_facet_columns(panel_count: usize, plot: Rect) -> usize {
    let aspect = (plot.width / plot.height.max(1.0)).clamp(0.25, 4.0);
    // Start from a compact near-square grid, then widen only when the viewport
    // aspect is strong enough to cross the next integer column count.
    let square_columns = (panel_count as f64).sqrt().ceil() as usize;
    ((panel_count as f64 * aspect).sqrt().floor() as usize)
        .max(square_columns.max(1))
        .min(panel_count.max(1))
}
