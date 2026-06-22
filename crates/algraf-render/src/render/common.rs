use algraf_core::{codes, Diagnostic};
use algraf_data::{DataType, Table};
use algraf_semantics::{AxisSelectorIr, FrameIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, ThemeIr};

use crate::helpers::{frame_axis, vector_column};
use crate::scale::categorical_domain;
use crate::theme::Theme;

/// Resolve a per-space theme, applying space-local overrides on top of the base.
/// CLI `--theme` (passed as `cli_override`) is the strongest source and is
/// applied last (spec §22.3).
pub(super) fn resolve_space_theme(
    base: &Theme,
    space_theme: Option<&ThemeIr>,
    cli_override: Option<&str>,
) -> Theme {
    let mut theme = base.clone();
    if let Some(ir) = space_theme {
        // A space theme starts from its own named base if it gives one, or else
        // inherits the chart base, then layers its overrides (spec §7.3, §20.8).
        let mut t = match &ir.base {
            Some(name) => Theme::by_name(name),
            None => base.clone(),
        };
        t.apply_overrides(&ir.overrides);
        theme = t;
    }
    if let Some(name) = cli_override {
        theme = Theme::by_name(name);
    }
    theme
}

pub(super) fn merged_scales(chart_scales: &[ScaleIr], space_scales: &[ScaleIr]) -> Vec<ScaleIr> {
    chart_scales
        .iter()
        .chain(space_scales.iter())
        .cloned()
        .collect()
}

pub(super) fn validate_scale_configs(
    frame: &FrameIr,
    table: &dyn Table,
    scales: &[ScaleIr],
    span: algraf_core::Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for scale in scales {
        let ScaleTargetIr::Axis(axis) = &scale.target else {
            continue;
        };
        let Some(axis_frame) = frame_axis(frame, *axis) else {
            continue;
        };
        if let Some(declared) = &scale.categorical_domain {
            validate_categorical_domain(scale, axis_frame, table, declared, diagnostics);
        }
        if scale.scale_type == Some(ScaleTypeIr::Categorical) {
            let Some(column) = vector_column(axis_frame) else {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "categorical scale requires a scalar position axis",
                    scale.span,
                ));
                continue;
            };
            if column.dtype == DataType::Geometry {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "categorical scale cannot be applied to a geometry axis",
                    column.span,
                ));
            }
            if scale.domain.is_some() {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "categorical scale cannot use numeric domain bounds",
                    scale.span,
                ));
            }
            if scale.breaks.is_some() {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "categorical scale cannot use numeric breaks",
                    scale.span,
                ));
            }
            if scale.integer.is_some() {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "categorical scale cannot use integer tick constraint",
                    scale.span,
                ));
            }
        }
        if scale.scale_type == Some(ScaleTypeIr::Temporal) {
            let column = axis_outer_vector_column(axis_frame);
            let temporal_column = column.is_some_and(|column| {
                matches!(
                    column.dtype,
                    algraf_data::DataType::Temporal | algraf_data::DataType::Unknown
                )
            });
            if !temporal_column {
                // An explicit temporal scale must not coerce strings or
                // numbers into dates at render time (spec §16.11); diagnose
                // and fall back to the column's natural axis.
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "temporal scale requires a temporal axis column; parse the column as a date or datetime first",
                    column.map(|column| column.span).unwrap_or(scale.span),
                ));
            }
        }
        if let Some(interval) = scale.tick_interval {
            let column = axis_outer_vector_column(axis_frame);
            let temporal_column = column.is_some_and(|column| {
                matches!(
                    column.dtype,
                    algraf_data::DataType::Temporal | algraf_data::DataType::Unknown
                )
            });
            if !temporal_column {
                diagnostics.push(Diagnostic::warning(
                    codes::E1608,
                    "`tickInterval` applies only to temporal axes; this axis is not temporal",
                    scale.span,
                ));
            } else if let Some(column) = column {
                // Warn when the requested cadence exceeds the interval tick
                // budget and the step count was promoted (spec §16.11).
                if let Some((min, max, _)) = crate::scale::temporal_domain(table, &column.name) {
                    let effective =
                        crate::space::temporal::interval_effective_count(min, max, interval);
                    if effective != i64::from(interval.count) {
                        diagnostics.push(Diagnostic::warning(
                            codes::R0004,
                            format!(
                                "`tickInterval` cadence was promoted from every {} {unit}(s) to every {effective} {unit}(s) to fit the tick budget",
                                interval.count,
                                unit = interval.unit.as_str(),
                            ),
                            scale.span,
                        ));
                    }
                }
            }
        }
        if matches!(
            scale.scale_type,
            Some(ScaleTypeIr::Log10) | Some(ScaleTypeIr::Sqrt)
        ) {
            let type_name = scale.scale_type.map(|t| t.as_str()).unwrap_or("");
            let Some(column) = vector_column(axis_frame) else {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    format!("{type_name} scale requires a continuous numeric axis"),
                    scale.span,
                ));
                continue;
            };
            if !matches!(
                column.dtype,
                algraf_data::DataType::Integer | algraf_data::DataType::Float
            ) {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    format!("{type_name} scale requires a continuous numeric axis"),
                    column.span,
                ));
            }
        }
        if let Some([a, b]) = scale.domain {
            if let (Some(a), Some(b)) = (a, b) {
                if (a - b).abs() <= f64::EPSILON {
                    diagnostics.push(Diagnostic::warning(
                        codes::R0004,
                        "scale domain endpoints must be distinct",
                        scale.span,
                    ));
                }
            }
            if scale.scale_type == Some(ScaleTypeIr::Log10)
                && [a, b].into_iter().flatten().any(|bound| bound <= 0.0)
            {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "log10 scale domain must be positive",
                    scale.span,
                ));
            }
            if scale.scale_type == Some(ScaleTypeIr::Sqrt)
                && [a, b].into_iter().flatten().any(|bound| bound < 0.0)
            {
                diagnostics.push(Diagnostic::warning(
                    codes::R0004,
                    "sqrt scale domain must be non-negative",
                    scale.span,
                ));
            }
        }
    }

    if frame_axis(frame, AxisSelectorIr::X).is_none()
        && frame_axis(frame, AxisSelectorIr::Y).is_none()
    {
        diagnostics.push(Diagnostic::warning(
            codes::R0004,
            "scale declarations could not be matched to this space",
            span,
        ));
    }
}

fn axis_outer_vector_column(frame: &FrameIr) -> Option<&algraf_semantics::ColumnRef> {
    match frame {
        FrameIr::Nested { outer, .. } => vector_column(outer),
        _ => vector_column(frame),
    }
}

fn validate_categorical_domain(
    scale: &ScaleIr,
    axis_frame: &FrameIr,
    table: &dyn Table,
    declared: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(column) = categorical_axis_column(
        axis_frame,
        scale.scale_type == Some(ScaleTypeIr::Categorical),
    ) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0004,
            "string-array domain applies only to categorical position axes",
            scale.span,
        ));
        return;
    };
    let observed = categorical_domain(table, &column.name);
    let appended: Vec<String> = observed
        .into_iter()
        .filter(|category| !declared.contains(category))
        .collect();
    if !appended.is_empty() {
        diagnostics.push(Diagnostic::warning(
            codes::R0004,
            format!(
                "data categories not listed in explicit domain were appended: {}",
                appended.join(", ")
            ),
            scale.span,
        ));
    }
}

fn categorical_axis_column(
    frame: &FrameIr,
    force_categorical: bool,
) -> Option<&algraf_semantics::ColumnRef> {
    match frame {
        FrameIr::Vector(column)
            if column.dtype == DataType::Unknown
                || column.dtype.is_categorical()
                || (force_categorical && column.dtype != DataType::Geometry) =>
        {
            Some(column)
        }
        FrameIr::Nested { outer, .. } => categorical_axis_column(outer, force_categorical),
        _ => None,
    }
}
