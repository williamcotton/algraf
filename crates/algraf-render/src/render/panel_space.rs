use algraf_semantics::ChartIr;

use crate::domains::SpaceDomainHints;
use crate::layout::{AxisSides, GuideExtra, Layout, LegendSize, Margins, Rect};
use crate::space::ScaledSpace;
use crate::theme::Theme;

use algraf_semantics::{FrameIr, SpaceIr};

#[allow(clippy::too_many_arguments)]
pub(super) fn compute_layout(
    ir: &ChartIr,
    width: f64,
    height: f64,
    has_legends: bool,
    has_axes: bool,
    top_extra: f64,
    bottom_extra: f64,
    guide_extra: GuideExtra,
    sides: AxisSides,
    margins: Margins,
    grid_categories: Option<&(Vec<String>, Vec<String>)>,
    facet_panel_count: Option<usize>,
    theme: &Theme,
    legend_size: Option<LegendSize>,
) -> Layout {
    if let Some((row_categories, col_categories)) = grid_categories {
        return Layout::compute_facet_grid_with_text_and_legend_size(
            width,
            height,
            has_legends,
            has_axes,
            row_categories.len().max(1),
            col_categories.len().max(1),
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            theme.legend_position,
            ir.layout.panel_spacing,
            legend_size,
        );
    }
    match facet_panel_count {
        Some(count) => Layout::compute_facets_with_text_and_legend_size(
            width,
            height,
            has_legends,
            has_axes,
            count,
            ir.layout.facet_columns,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            theme.legend_position,
            ir.layout.panel_spacing,
            legend_size,
        ),
        None => Layout::compute_with_text_and_legend_size(
            width,
            height,
            has_legends,
            has_axes,
            top_extra,
            bottom_extra,
            guide_extra,
            sides,
            margins,
            theme.legend_position,
            legend_size,
        ),
    }
}

pub(super) fn build_cartesian_scaled(
    frame: &FrameIr,
    x_table: &dyn algraf_data::Table,
    y_table: &dyn algraf_data::Table,
    plot: Rect,
    hints: &SpaceDomainHints,
    scales: &[algraf_semantics::ScaleIr],
    space: &SpaceIr,
) -> Option<(ScaledSpace, Rect)> {
    let x_range = (plot.x, plot.right());
    let y_range = (plot.bottom(), plot.y);
    let scaled = ScaledSpace::build_with_axis_tables(
        frame, x_table, y_table, x_range, y_range, hints, scales, space.view,
    )?;
    let Some(aspect) = space.view.aspect else {
        return Some((scaled, plot));
    };
    let adjusted = scaled.fixed_aspect_plot(plot, aspect);
    if adjusted == plot {
        return Some((scaled, plot));
    }
    let adjusted_scaled = ScaledSpace::build_with_axis_tables(
        frame,
        x_table,
        y_table,
        (adjusted.x, adjusted.right()),
        (adjusted.bottom(), adjusted.y),
        hints,
        scales,
        space.view,
    )?;
    Some((adjusted_scaled, adjusted))
}
