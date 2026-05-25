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
use algraf_semantics::{BinClosedIr, ChartIr, FrameIr, SpaceDataRef, StatOptionsIr};

use crate::stats;

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
                } => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        let options = stats::BinOptions {
                            bins: bins_or_default(*bins),
                            bin_width: bin_width.filter(|n| *n > 0.0),
                            boundary: *boundary,
                            closed: match closed {
                                BinClosedIr::Left => stats::BinClosed::Left,
                                BinClosedIr::Right => stats::BinClosed::Right,
                            },
                        };
                        Some(stats::bin_with_options(source, &col.name, options))
                    } else {
                        None
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
                    if let FrameIr::Vector(col) = &d.stat.input {
                        let options = stats::DensityOptions {
                            bandwidth: bandwidth.filter(|n| *n > 0.0),
                            grid_points: grid_points
                                .filter(|n| *n >= 2.0)
                                .map(|n| n.round() as usize)
                                .unwrap_or(256),
                        };
                        Some(stats::density(source, &col.name, options))
                    } else {
                        None
                    }
                }
                StatOptionsIr::Smooth { .. } => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::smooth_lm(source, &x.name, &y.name))
                        } else {
                            None
                        }
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

/// Resolve a `bins` option to a positive integer, falling back to the default
/// of 30 when unset or out of range (spec §15.x).
fn bins_or_default(bins: Option<f64>) -> usize {
    bins.filter(|n| *n >= 1.0)
        .map(|n| n.round() as usize)
        .unwrap_or(30)
}
