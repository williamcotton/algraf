//! Viewport layout with fixed margins (spec §17).

use algraf_semantics::{AxisPositionIr, LegendPositionIr, PanelSpacingIr};

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

/// Measured legend content size in pixels, excluding the gap between the legend
/// and the plot panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegendSize {
    pub width: f64,
    pub height: f64,
}

/// Which side each axis is drawn on, so the layout reserves the larger axis
/// margin on the chosen side (spec §17.2–§17.3, §19.2–§19.3). The defaults
/// (`y_right = false`, `x_top = false`) keep the y axis at the left and the x
/// axis at the bottom.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AxisSides {
    pub y_right: bool,
    pub x_top: bool,
}

impl AxisSides {
    pub fn from_positions(y: AxisPositionIr, x: AxisPositionIr) -> AxisSides {
        AxisSides {
            y_right: matches!(y, AxisPositionIr::Right),
            x_top: matches!(x, AxisPositionIr::Top),
        }
    }
}

/// Per-side extra reservation in pixels for guide tick labels/titles. The chart
/// title and caption reserves are passed separately as `top_extra`/`bottom_extra`
/// and always stay on the top/bottom; these route to whichever side carries the
/// axis (spec §17.3).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GuideExtra {
    /// y-axis tick-label/title width, reserved on the y-axis side.
    pub y: f64,
    /// x-axis tick-label/title height, reserved on the x-axis side.
    pub x: f64,
}

/// The larger axis-bearing margin and the smaller opposite margin per dimension.
const MARGIN_TOP: f64 = 40.0;
const MARGIN_RIGHT: f64 = 30.0;
pub(crate) const MARGIN_BOTTOM: f64 = 50.0;
pub(crate) const MARGIN_LEFT: f64 = 60.0;
/// Margin reserved on the side carrying the x axis (its line, tick labels, and
/// title floor), versus the opposite side which is light padding.
const X_AXIS_MARGIN: f64 = MARGIN_BOTTOM;
const X_OPPOSITE_MARGIN: f64 = MARGIN_TOP;
/// Margin reserved on the side carrying the y axis, versus the opposite side.
const Y_AXIS_MARGIN: f64 = MARGIN_LEFT;
const Y_OPPOSITE_MARGIN: f64 = MARGIN_RIGHT;
/// Base padding per side when the chart has no axes (e.g. the `void` theme).
/// Unlike the axis margins this is pure padding, so a configured `margin*`
/// value overrides it outright — down to 0 — rather than acting as a floor.
const NO_AXES_MARGIN: f64 = 10.0;
const LEGEND_WIDTH: f64 = 120.0;
const LEGEND_HEIGHT: f64 = 72.0;
const LEGEND_GAP: f64 = 16.0;
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
            GuideExtra::default(),
            AxisSides::default(),
            Margins::default(),
            LegendPositionIr::Right,
        )
    }

    /// Compute layout with extra title/caption reserve. `guide_extra` reserves
    /// room for axis tick labels on whichever side `sides` puts each axis (spec
    /// §17.3). `margins` applies per-side user minimums on top of the computed
    /// margins.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_with_text(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        top_extra: f64,
        bottom_extra: f64,
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
    ) -> Layout {
        Layout::compute_with_text_and_legend_size(
            width,
            height,
            has_legend,
            has_axes,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            legend_position,
            None,
        )
    }

    /// Compute layout with extra title/caption reserve and measured legend
    /// content size. The legend size excludes the plot/legend gap; the layout
    /// adds that gap on the side carrying the legend.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_with_text_and_legend_size(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        top_extra: f64,
        bottom_extra: f64,
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
        legend_size: Option<LegendSize>,
    ) -> Layout {
        // The axis-bearing side reserves the larger margin; its opposite side is
        // light padding. Axis side moves the reservation, not the data marks
        // (spec §17.2–§17.3, §19.2–§19.3).
        let (base_top, base_right, base_bottom, base_left) = if has_axes {
            (
                if sides.x_top {
                    X_AXIS_MARGIN
                } else {
                    X_OPPOSITE_MARGIN
                },
                if sides.y_right {
                    Y_AXIS_MARGIN
                } else {
                    Y_OPPOSITE_MARGIN
                },
                if sides.x_top {
                    X_OPPOSITE_MARGIN
                } else {
                    X_AXIS_MARGIN
                },
                if sides.y_right {
                    Y_OPPOSITE_MARGIN
                } else {
                    Y_AXIS_MARGIN
                },
            )
        } else {
            (
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
                NO_AXES_MARGIN,
            )
        };
        // Content reserve for chart title/subtitle (top) and caption/source
        // (bottom). This is a hard minimum that an explicit margin never clips.
        let top_extra = top_extra.max(0.0);
        let bottom_extra = bottom_extra.max(0.0);
        // Guide tick-label/title reserve routed to whichever side carries the
        // axis. With a right y axis the label width reserves on the right; with a
        // top x axis the label height reserves on the top.
        let guide_x = guide_extra.x.max(0.0);
        let guide_y = guide_extra.y.max(0.0);
        let extra_top = top_extra + if sides.x_top { guide_x } else { 0.0 };
        let extra_bottom = bottom_extra + if sides.x_top { 0.0 } else { guide_x };
        let extra_left = if sides.y_right { 0.0 } else { guide_y };
        let extra_right = if sides.y_right { guide_y } else { 0.0 };
        // Computed default margins = base padding + content reserve.
        let computed_top = base_top + extra_top;
        let computed_right = base_right + extra_right;
        let computed_bottom = base_bottom + extra_bottom;
        let computed_left = base_left + extra_left;
        let (top, right, bottom, left) = if has_axes {
            // With axes the base margin holds the axis line and tick labels, so
            // a configured value acts as a floor: it can widen a side but never
            // shrink below what the guides require (spec §17.3).
            (
                margins.top.map_or(computed_top, |m| computed_top.max(m)),
                margins
                    .right
                    .map_or(computed_right, |m| computed_right.max(m)),
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
                margins.top.map_or(computed_top, |m| m.max(extra_top)),
                margins.right.unwrap_or(computed_right),
                margins
                    .bottom
                    .map_or(computed_bottom, |m| m.max(extra_bottom)),
                margins.left.map_or(computed_left, |m| m.max(extra_left)),
            )
        };
        let (legend_left, legend_right, legend_top, legend_bottom) =
            legend_reserve(has_legend, legend_position, legend_size);
        let legend_size = measured_legend_size(legend_size);

        let plot = Rect {
            x: left + legend_left,
            y: top + legend_top,
            width: (width - left - right - legend_left - legend_right).max(1.0),
            height: (height - top - bottom - legend_top - legend_bottom).max(1.0),
        };

        let vertical_legend_svg_height = if has_legend
            && matches!(
                legend_position,
                LegendPositionIr::Right | LegendPositionIr::Left
            ) {
            plot.y + legend_size.height + bottom
        } else {
            height
        };

        let legend = has_legend.then(|| match legend_position {
            LegendPositionIr::Right => Rect {
                x: plot.right() + LEGEND_GAP,
                y: plot.y,
                width: legend_size.width,
                height: legend_size.height,
            },
            LegendPositionIr::Left => Rect {
                x: left,
                y: plot.y,
                width: legend_size.width,
                height: legend_size.height,
            },
            LegendPositionIr::Bottom => Rect {
                x: plot.x,
                y: plot.bottom() + bottom,
                width: plot.width,
                height: legend_size.height,
            },
            LegendPositionIr::Top => Rect {
                x: plot.x,
                y: top,
                width: plot.width,
                height: legend_size.height,
            },
        });

        Layout {
            svg: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: height.max(vertical_legend_svg_height),
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
            GuideExtra::default(),
            AxisSides::default(),
            Margins::default(),
            LegendPositionIr::Right,
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
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
        panel_spacing: Option<PanelSpacingIr>,
    ) -> Layout {
        Layout::compute_facets_with_text_and_legend_size(
            width,
            height,
            has_legend,
            has_axes,
            panel_count,
            columns,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            legend_position,
            panel_spacing,
            None,
        )
    }

    /// Compute a facet-wrap layout with extra title/caption reserve and measured
    /// legend content size.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_facets_with_text_and_legend_size(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        panel_count: usize,
        columns: Option<usize>,
        top_extra: f64,
        bottom_extra: f64,
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
        panel_spacing: Option<PanelSpacingIr>,
        legend_size: Option<LegendSize>,
    ) -> Layout {
        let mut layout = Layout::compute_with_text_and_legend_size(
            width,
            height,
            has_legend,
            has_axes,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            legend_position,
            legend_size,
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
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
        panel_spacing: Option<PanelSpacingIr>,
    ) -> Layout {
        Layout::compute_facet_grid_with_text_and_legend_size(
            width,
            height,
            has_legend,
            has_axes,
            rows,
            columns,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            legend_position,
            panel_spacing,
            None,
        )
    }

    /// Compute an exact row-by-column facet-grid layout with measured legend
    /// content size.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_facet_grid_with_text_and_legend_size(
        width: f64,
        height: f64,
        has_legend: bool,
        has_axes: bool,
        rows: usize,
        columns: usize,
        top_extra: f64,
        bottom_extra: f64,
        guide_extra: GuideExtra,
        sides: AxisSides,
        margins: Margins,
        legend_position: LegendPositionIr,
        panel_spacing: Option<PanelSpacingIr>,
        legend_size: Option<LegendSize>,
    ) -> Layout {
        let mut layout = Layout::compute_with_text_and_legend_size(
            width,
            height,
            has_legend,
            has_axes,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            legend_position,
            legend_size,
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

fn measured_legend_size(size: Option<LegendSize>) -> LegendSize {
    let default = LegendSize {
        width: LEGEND_WIDTH - LEGEND_GAP,
        height: LEGEND_HEIGHT - LEGEND_GAP,
    };
    match size {
        Some(size) => LegendSize {
            width: size.width.max(default.width).max(1.0),
            height: size.height.max(default.height).max(1.0),
        },
        None => default,
    }
}

fn legend_reserve(
    has_legend: bool,
    position: LegendPositionIr,
    size: Option<LegendSize>,
) -> (f64, f64, f64, f64) {
    if !has_legend {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let size = measured_legend_size(size);
    match position {
        LegendPositionIr::Right => (0.0, size.width + LEGEND_GAP, 0.0, 0.0),
        LegendPositionIr::Left => (size.width + LEGEND_GAP, 0.0, 0.0, 0.0),
        LegendPositionIr::Top => (0.0, 0.0, size.height + LEGEND_GAP, 0.0),
        LegendPositionIr::Bottom => (0.0, 0.0, 0.0, size.height + LEGEND_GAP),
    }
}
