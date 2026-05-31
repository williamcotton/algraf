//! Derived-table execution: the data side of the render planning boundary
//! (spec §15, §24.6).
//!
//! This is where statistical transforms consume loaded data. [`compute_derived`]
//! runs each `Derive`/named-table stat once, eagerly, against an input resolved
//! through the [`Table`] trait, and materializes the result into an owned
//! [`DataFrame`]. Those frames are keyed by name and later resolved by
//! [`active_table`] when a space references them. All execution happens during
//! planning; nothing here writes output, and stats read only through [`Table`],
//! never through concrete dataframe internals.

use std::collections::HashMap;

use algraf_data::{DataFrame, Table};
use algraf_semantics::{
    BinClosedIr, BinIntervalIr, ChartIr, FrameIr, GridBinsIr, IntervalOrientationIr, LevelSpecIr,
    SmoothMethodIr, SpaceDataRef, StatOptionsIr, StepDirectionIr, SummaryReducerIr,
};

use crate::stats;

/// Translate IR smooth options into renderer [`stats::SmoothOptions`], applying
/// the loess-span default (spec §15.x).
pub(super) fn smooth_options(
    method: SmoothMethodIr,
    span: Option<f64>,
    se: bool,
) -> stats::SmoothOptions {
    let defaults = stats::SmoothOptions::default();
    stats::SmoothOptions {
        method: match method {
            SmoothMethodIr::Lm => stats::SmoothMethod::Lm,
            SmoothMethodIr::Loess => stats::SmoothMethod::Loess,
        },
        span: span.unwrap_or(defaults.span),
        se,
        ..defaults
    }
}

pub(super) fn active_table<'t>(
    data: &SpaceDataRef,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
) -> &'t dyn Table {
    match data {
        SpaceDataRef::Primary => primary,
        // Named tables are seeded into the same map as derived tables, so both
        // resolve the same way (spec §10.x).
        SpaceDataRef::Derived(name) | SpaceDataRef::Table(name) => derived
            .get(name)
            .map(|d| d as &dyn Table)
            .unwrap_or(primary),
    }
}

pub(super) fn compute_derived(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
) -> HashMap<String, DataFrame> {
    // Seed with the chart's named CSV tables; derived stats may read from them
    // and `SpaceDataRef::Table` resolves through this same map.
    let mut derived: HashMap<String, DataFrame> = named_tables.clone();
    for d in &ir.derived_tables {
        let frame = {
            let source = active_table(&d.data, primary, &derived);
            match &d.stat.options {
                StatOptionsIr::Bin {
                    bins,
                    bin_width,
                    boundary,
                    closed,
                    interval,
                } => {
                    let options = stats::BinOptions {
                        bins: bins_or_default(*bins),
                        bin_width: bin_width.filter(|n| *n > 0.0),
                        boundary: *boundary,
                        closed: match closed {
                            BinClosedIr::Left => stats::BinClosed::Left,
                            BinClosedIr::Right => stats::BinClosed::Right,
                        },
                        interval: interval.map(render_bin_interval),
                    };
                    match &d.stat.input {
                        FrameIr::Vector(col) => {
                            Some(stats::bin_with_options(source, &col.name, options))
                        }
                        // A grouped histogram desugars to a two-column Bin input
                        // `(value, group)`, producing pre-stacked per-group bins
                        // (spec §15.6).
                        FrameIr::Cartesian(cols) => {
                            if let (Some(FrameIr::Vector(value)), Some(FrameIr::Vector(group))) =
                                (cols.first(), cols.get(1))
                            {
                                Some(stats::bin_grouped(
                                    source,
                                    &value.name,
                                    &group.name,
                                    options,
                                ))
                            } else {
                                None
                            }
                        }
                        // A blended histogram desugars to a union of numeric
                        // columns, producing one overlaid series per member.
                        FrameIr::Union(members) => {
                            let columns: Option<Vec<&str>> = members
                                .iter()
                                .map(|member| match member {
                                    FrameIr::Vector(column) => Some(column.name.as_str()),
                                    _ => None,
                                })
                                .collect();
                            columns.map(|columns| stats::bin_blended(source, &columns, options))
                        }
                        _ => None,
                    }
                }
                StatOptionsIr::Bin2D { bins } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::bin2d(
                                source,
                                &x.name,
                                &y.name,
                                stats::Bin2DOptions {
                                    bins: bins_or_default(*bins),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::HexBin { bins } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::hexbin_frame(
                                source,
                                &x.name,
                                &y.name,
                                stats::Bin2DOptions {
                                    bins: bins_or_default(*bins),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Summary2D { bins, reducer } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x)),
                            Some(FrameIr::Vector(y)),
                            Some(FrameIr::Vector(z)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::summary2d(
                                source,
                                &x.name,
                                &y.name,
                                &z.name,
                                stats::Summary2DOptions {
                                    bins: render_grid_size(*bins, 30),
                                    reducer: render_summary_reducer(*reducer),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::SummaryHex { bins, reducer } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x)),
                            Some(FrameIr::Vector(y)),
                            Some(FrameIr::Vector(z)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::summaryhex(
                                source,
                                &x.name,
                                &y.name,
                                &z.name,
                                stats::Summary2DOptions {
                                    bins: stats::GridSize::square(bins_or_default(*bins)),
                                    reducer: render_summary_reducer(*reducer),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::ContourLines { levels } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x)),
                            Some(FrameIr::Vector(y)),
                            Some(FrameIr::Vector(z)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::contour_lines(
                                source,
                                &x.name,
                                &y.name,
                                &z.name,
                                stats::ContourOptions {
                                    levels: render_level_spec(levels),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::ContourBands { levels } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x)),
                            Some(FrameIr::Vector(y)),
                            Some(FrameIr::Vector(z)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::contour_bands(
                                source,
                                &x.name,
                                &y.name,
                                &z.name,
                                stats::ContourOptions {
                                    levels: render_level_spec(levels),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Density2D { bandwidth, grid } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::density2d(
                                source,
                                &x.name,
                                &y.name,
                                stats::Density2DOptions {
                                    bandwidth: *bandwidth,
                                    grid: render_grid_size(*grid, 64),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Density2DContours {
                    bandwidth,
                    grid,
                    levels,
                } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::density2d_contours(
                                source,
                                &x.name,
                                &y.name,
                                stats::Density2DOptions {
                                    bandwidth: *bandwidth,
                                    grid: render_grid_size(*grid, 64),
                                },
                                stats::ContourOptions {
                                    levels: render_level_spec(levels),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Density2DBands {
                    bandwidth,
                    grid,
                    levels,
                } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::density2d_bands(
                                source,
                                &x.name,
                                &y.name,
                                stats::Density2DOptions {
                                    bandwidth: *bandwidth,
                                    grid: render_grid_size(*grid, 64),
                                },
                                stats::ContourOptions {
                                    levels: render_level_spec(levels),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Count => {
                    let mut group_cols: Vec<&str> = Vec::new();
                    match &d.stat.input {
                        FrameIr::Vector(col) => group_cols.push(&col.name),
                        FrameIr::Nested { outer, inner } => {
                            if let (FrameIr::Vector(o), FrameIr::Vector(i)) =
                                (outer.as_ref(), inner.as_ref())
                            {
                                group_cols.push(&o.name);
                                group_cols.push(&i.name);
                            }
                        }
                        _ => {}
                    }
                    if group_cols.is_empty() {
                        None
                    } else {
                        Some(stats::count_by(source, &group_cols))
                    }
                }
                StatOptionsIr::Density {
                    bandwidth,
                    grid_points,
                } => {
                    let options = stats::DensityOptions {
                        bandwidth: bandwidth.filter(|n| *n > 0.0),
                        grid_points: grid_points
                            .filter(|n| *n >= 2.0)
                            .map(|n| n.round() as usize)
                            .unwrap_or(256),
                    };
                    match &d.stat.input {
                        FrameIr::Vector(col) => Some(stats::density(source, &col.name, options)),
                        FrameIr::Union(members) => {
                            let columns: Option<Vec<&str>> = members
                                .iter()
                                .map(|member| match member {
                                    FrameIr::Vector(column) => Some(column.name.as_str()),
                                    _ => None,
                                })
                                .collect();
                            columns.map(|columns| stats::density_blended(source, &columns, options))
                        }
                        _ => None,
                    }
                }
                StatOptionsIr::Smooth { method, span, se } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            let options = smooth_options(*method, *span, *se);
                            Some(stats::smooth(source, &x.name, &y.name, options))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::StepVertices { direction } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::step_vertices(
                                source,
                                &x.name,
                                &y.name,
                                stats::StepVerticesOptions {
                                    direction: match direction {
                                        StepDirectionIr::Hv => stats::StepDirection::Hv,
                                        StepDirectionIr::Vh => stats::StepDirection::Vh,
                                    },
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::VectorEndpoints { length_scale } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x)),
                            Some(FrameIr::Vector(y)),
                            Some(FrameIr::Vector(angle)),
                            Some(FrameIr::Vector(length)),
                        ) = (cols.first(), cols.get(1), cols.get(2), cols.get(3))
                        {
                            Some(stats::vector_endpoints(
                                source,
                                &x.name,
                                &y.name,
                                &angle.name,
                                &length.name,
                                stats::VectorEndpointsOptions {
                                    length_scale: length_scale.unwrap_or(1.0),
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::CurveSample { curvature, points } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(x0)),
                            Some(FrameIr::Vector(y0)),
                            Some(FrameIr::Vector(x1)),
                            Some(FrameIr::Vector(y1)),
                        ) = (cols.first(), cols.get(1), cols.get(2), cols.get(3))
                        {
                            Some(stats::curve_sample(
                                source,
                                &x0.name,
                                &y0.name,
                                &x1.name,
                                &y1.name,
                                stats::CurveSampleOptions {
                                    curvature: *curvature,
                                    points: *points,
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::IntervalSegments {
                    orientation,
                    cap_width,
                } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(position)),
                            Some(FrameIr::Vector(lower)),
                            Some(FrameIr::Vector(upper)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::interval_segments(
                                source,
                                &position.name,
                                &lower.name,
                                &upper.name,
                                stats::IntervalSegmentsOptions {
                                    orientation: render_interval_orientation(*orientation),
                                    cap_width: *cap_width,
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::IntervalRects { orientation, width } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (
                            Some(FrameIr::Vector(position)),
                            Some(FrameIr::Vector(lower)),
                            Some(FrameIr::Vector(upper)),
                        ) = (cols.first(), cols.get(1), cols.get(2))
                        {
                            Some(stats::interval_rects(
                                source,
                                &position.name,
                                &lower.name,
                                &upper.name,
                                stats::IntervalWidthOptions {
                                    orientation: render_interval_orientation(*orientation),
                                    width: *width,
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::IntervalMiddles { orientation, width } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(position)), Some(FrameIr::Vector(middle))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::interval_middles(
                                source,
                                &position.name,
                                &middle.name,
                                stats::IntervalWidthOptions {
                                    orientation: render_interval_orientation(*orientation),
                                    width: *width,
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatOptionsIr::Centroid => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        Some(crate::geo_stats::centroid(source, &col.name))
                    } else {
                        None
                    }
                }
                StatOptionsIr::Simplify { tolerance } => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        // Default tolerance: a small fraction of a degree, fine
                        // enough to keep shapes recognizable (spec §15.13).
                        let tol = tolerance.filter(|t| *t >= 0.0).unwrap_or(0.01);
                        Some(crate::geo_stats::simplify(source, &col.name, tol))
                    } else {
                        None
                    }
                }
                StatOptionsIr::SpatialJoin { table, .. } => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        // The polygon table is a chart-scoped named table, so it
                        // is already materialized in `derived` (spec §15.14).
                        derived.get(table).map(|polygon| {
                            crate::geo_stats::spatial_join_within(source, &col.name, polygon)
                        })
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(frame) = frame {
            derived.insert(d.name.clone(), frame);
        }
    }
    derived
}

fn render_interval_orientation(orientation: IntervalOrientationIr) -> stats::IntervalOrientation {
    match orientation {
        IntervalOrientationIr::Vertical => stats::IntervalOrientation::Vertical,
        IntervalOrientationIr::Horizontal => stats::IntervalOrientation::Horizontal,
    }
}

fn render_bin_interval(interval: BinIntervalIr) -> stats::BinInterval {
    match interval {
        BinIntervalIr::Minute => stats::BinInterval::Minute,
        BinIntervalIr::Hour => stats::BinInterval::Hour,
        BinIntervalIr::Day => stats::BinInterval::Day,
        BinIntervalIr::Week => stats::BinInterval::Week,
        BinIntervalIr::Month => stats::BinInterval::Month,
        BinIntervalIr::Quarter => stats::BinInterval::Quarter,
        BinIntervalIr::Year => stats::BinInterval::Year,
    }
}

fn render_grid_size(grid: GridBinsIr, default: usize) -> stats::GridSize {
    let x = grid
        .x
        .filter(|n| *n >= 1.0)
        .map(|n| n.round() as usize)
        .unwrap_or(default);
    let y = grid
        .y
        .filter(|n| *n >= 1.0)
        .map(|n| n.round() as usize)
        .unwrap_or(default);
    stats::GridSize { x, y }
}

fn render_level_spec(levels: &LevelSpecIr) -> stats::LevelSpec {
    match levels {
        LevelSpecIr::Count(count) => {
            stats::LevelSpec::Count(count.filter(|n| *n >= 1.0).map(|n| n.round() as usize))
        }
        LevelSpecIr::Values(values) => stats::LevelSpec::Values(values.clone()),
    }
}

fn render_summary_reducer(reducer: SummaryReducerIr) -> stats::SummaryReducer {
    match reducer {
        SummaryReducerIr::Count => stats::SummaryReducer::Count,
        SummaryReducerIr::Mean => stats::SummaryReducer::Mean,
        SummaryReducerIr::Min => stats::SummaryReducer::Min,
        SummaryReducerIr::Max => stats::SummaryReducer::Max,
        SummaryReducerIr::Sum => stats::SummaryReducer::Sum,
        SummaryReducerIr::Median => stats::SummaryReducer::Median,
    }
}

/// Resolve a `bins` option to a positive integer, falling back to the default
/// of 30 when unset or out of range (spec §15.x).
fn bins_or_default(bins: Option<f64>) -> usize {
    bins.filter(|n| *n >= 1.0)
        .map(|n| n.round() as usize)
        .unwrap_or(30)
}
