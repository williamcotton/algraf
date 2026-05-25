use std::collections::HashMap;

use algraf_data::{DataFrame, Table};
use algraf_semantics::{
    ChartIr, FrameIr, GeometryIr, GeometryKind, GradientIr, PropertyKey, ScaleIr, ScaleTargetIr,
    SettingValue,
};

use crate::aes::{color_spec, number_spec, ColorSpec, Legend, LegendKind};
use crate::geom::{DEFAULT_SIZE_RANGE, DEFAULT_STROKE_WIDTH_RANGE};
use crate::stats;
use crate::theme::Theme;

use super::common::merged_scales;
use super::derived::active_table;

/// Collect deduplicated fill/stroke/size legends across all spaces (spec §19.5).
pub(super) fn collect_legends(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    theme: &Theme,
) -> Vec<Legend> {
    // Candidate legends paired with the aesthetic that produced them, so a
    // fill legend and a stroke legend over the same column can be merged below.
    let mut candidates: Vec<(PropertyKey, Legend)> = Vec::new();
    for space in &ir.spaces {
        let guides = ir.guides.with_overrides(&space.guides);
        if !guides.legend {
            continue;
        }
        let scales = merged_scales(&ir.scales, &space.scales);
        let table = active_table(&space.data, primary, derived);
        for geo in &space.geometries {
            for aesthetic in [PropertyKey::Fill, PropertyKey::Stroke] {
                if aesthetic == PropertyKey::Fill && !guides.fill_legend {
                    continue;
                }
                if aesthetic == PropertyKey::Stroke && !guides.stroke_legend {
                    continue;
                }
                if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
                    let spec = color_spec(geo, aesthetic, table, &scales);
                    // A `Scale(<aesthetic>: col, label: "...")` overrides the
                    // column-derived legend title (spec §16.13).
                    let title = scale_label(&scales, aesthetic.as_str())
                        .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
                    if let Some(legend) = spec.legend(&title) {
                        if !candidates
                            .iter()
                            .any(|(a, l)| *a == aesthetic && l.title == legend.title)
                        {
                            candidates.push((aesthetic, legend));
                        }
                    }
                }
            }
            // `HexBin` is rendered by a bespoke geometry rather than desugared
            // to a fill-mapped `Rect` (as `Bin2D` is), so it has no `fill`
            // mapping for the loop above to find. Synthesize the same continuous
            // count legend here, over the binned count domain (spec §19.5).
            if geo.kind == GeometryKind::HexBin && guides.fill_legend {
                if let Some(legend) = hexbin_count_legend(geo, &space.frame, table, &scales) {
                    if !candidates
                        .iter()
                        .any(|(a, l)| *a == PropertyKey::Fill && l.title == legend.title)
                    {
                        candidates.push((PropertyKey::Fill, legend));
                    }
                }
            }
            // Size legends for numeric aesthetics: `strokeWidth` (a line of the
            // mapped thickness) and `size` (a circle of the mapped radius). Each
            // is only produced when the aesthetic maps a column (spec §19.5).
            for (aesthetic, kind, default_range, constant_default) in [
                (
                    PropertyKey::StrokeWidth,
                    LegendKind::Width,
                    DEFAULT_STROKE_WIDTH_RANGE,
                    theme.line_width,
                ),
                (
                    PropertyKey::Size,
                    LegendKind::Radius,
                    DEFAULT_SIZE_RANGE,
                    theme.point_size,
                ),
            ] {
                let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) else {
                    continue;
                };
                let spec = number_spec(
                    geo,
                    aesthetic,
                    table,
                    &scales,
                    default_range,
                    constant_default,
                );
                let title = scale_label(&scales, aesthetic.as_str())
                    .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
                if let Some(legend) = spec.legend(&title, kind) {
                    if !candidates
                        .iter()
                        .any(|(a, l)| *a == aesthetic && l.title == legend.title)
                    {
                        candidates.push((aesthetic, legend));
                    }
                }
            }
        }
    }
    merge_fill_stroke_legends(candidates)
}

/// Build the continuous `count` legend for a bespoke `HexBin` geometry, unless
/// `fill` is set to a constant color. The count domain is derived by running
/// the same binning the renderer uses, so the legend's swatch colors match the
/// rendered hexagons.
fn hexbin_count_legend(
    geo: &GeometryIr,
    frame: &FrameIr,
    table: &dyn Table,
    scales: &[ScaleIr],
) -> Option<Legend> {
    if geo
        .settings
        .iter()
        .any(|s| s.name == PropertyKey::Fill && matches!(s.value, SettingValue::String(_)))
    {
        return None;
    }
    let FrameIr::Cartesian(axes) = frame else {
        return None;
    };
    let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1)) else {
        return None;
    };
    let bins = geo
        .settings
        .iter()
        .find(|s| s.name == PropertyKey::Bins)
        .and_then(|s| match s.value {
            SettingValue::Number(n) if n >= 1.0 => Some(n.round() as usize),
            _ => None,
        })
        .unwrap_or(30);
    let cells = stats::hexbin(table, &x.name, &y.name, stats::Bin2DOptions { bins });
    let min = cells.iter().map(|c| c.count).min()? as f64;
    let max = cells.iter().map(|c| c.count).max()? as f64;
    let stops = GradientIr::Even(
        crate::theme::CONTINUOUS_GRADIENT
            .iter()
            .map(|stop| (*stop).to_string())
            .collect(),
    );
    let spec = ColorSpec::Gradient {
        col: "count".to_string(),
        min,
        max,
        stops,
    };
    let title = scale_label(scales, "fill").unwrap_or_else(|| "count".to_string());
    spec.legend(&title)
}

/// The explicit `label` of a `fill`/`stroke` aesthetic scale, if declared.
fn scale_label(scales: &[ScaleIr], aesthetic: &str) -> Option<String> {
    scales.iter().find_map(|scale| match &scale.target {
        ScaleTargetIr::Aesthetic { aesthetic: a, .. } if a == aesthetic => scale.label.clone(),
        _ => None,
    })
}

/// Merge a `fill` legend and a `stroke` legend that share a title and have
/// compatible (identical, discrete) domains into a single legend whose swatches
/// show both colors (spec §19.7). Non-mergeable candidates pass through
/// unchanged, deduplicated by title with the first occurrence winning.
fn merge_fill_stroke_legends(candidates: Vec<(PropertyKey, Legend)>) -> Vec<Legend> {
    let mut out: Vec<Legend> = Vec::new();
    for (aesthetic, legend) in candidates {
        let Some(existing) = out.iter_mut().find(|l| l.title == legend.title) else {
            out.push(legend);
            continue;
        };

        // A fill/stroke pair over the same column with identical discrete entry
        // labels merges into one swatch set showing both colors. Only the first
        // unmerged base accepts a partner.
        let labels_match = existing.kind == LegendKind::Discrete
            && legend.kind == LegendKind::Discrete
            && existing.entries.len() == legend.entries.len()
            && existing
                .entries
                .iter()
                .zip(&legend.entries)
                .all(|(a, b)| a.0 == b.0);
        let new_colors: Vec<String> = legend.entries.iter().map(|(_, c)| c.clone()).collect();
        if labels_match && existing.stroke_entries.is_empty() {
            if aesthetic == PropertyKey::Stroke {
                // Existing is the fill base; this stroke legend adds outlines.
                existing.stroke_entries = new_colors;
            } else {
                // Existing was a stroke-only base seen first; promote the fill
                // colors to the swatch face and demote the strokes to outlines.
                existing.stroke_entries = existing.entries.iter().map(|(_, c)| c.clone()).collect();
                for (entry, color) in existing.entries.iter_mut().zip(new_colors) {
                    entry.1 = color;
                }
            }
        }
        // Otherwise the title already has a legend: keep the first.
    }
    out
}
