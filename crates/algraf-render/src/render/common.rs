use algraf_core::Diagnostic;
use algraf_semantics::{AxisSelectorIr, FrameIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, ThemeIr};

use crate::helpers::{frame_axis, vector_column};
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
        if scale.scale_type == Some(ScaleTypeIr::Log10) {
            let Some(column) = vector_column(axis_frame) else {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale requires a continuous numeric axis",
                    scale.span,
                ));
                continue;
            };
            if !matches!(
                column.dtype,
                algraf_data::DataType::Integer | algraf_data::DataType::Float
            ) {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale requires a continuous numeric axis",
                    column.span,
                ));
            }
        }
        if let Some([a, b]) = scale.domain {
            if let (Some(a), Some(b)) = (a, b) {
                if (a - b).abs() <= f64::EPSILON {
                    diagnostics.push(Diagnostic::warning(
                        "R0004",
                        "scale domain endpoints must be distinct",
                        scale.span,
                    ));
                }
            }
            if scale.scale_type == Some(ScaleTypeIr::Log10)
                && [a, b].into_iter().flatten().any(|bound| bound <= 0.0)
            {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale domain must be positive",
                    scale.span,
                ));
            }
        }
    }

    if frame_axis(frame, AxisSelectorIr::X).is_none()
        && frame_axis(frame, AxisSelectorIr::Y).is_none()
    {
        diagnostics.push(Diagnostic::warning(
            "R0004",
            "scale declarations could not be matched to this space",
            span,
        ));
    }
}
