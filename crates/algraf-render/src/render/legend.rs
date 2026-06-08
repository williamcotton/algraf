use algraf_data::Table;
use algraf_semantics::{
    FrameIr, GeometryIr, GeometryKind, GlyphScalePolicyIr, GradientIr, PropertyKey, ScaleIr,
    ScaleTargetIr, SettingValue,
};

use crate::aes::{color_spec, number_spec, ColorSpec, Legend, LegendImage, LegendKind};
use crate::geom::{DEFAULT_FILL, DEFAULT_SIZE_RANGE, DEFAULT_STROKE_WIDTH_RANGE};
use crate::marker::marker_for_index;
use crate::render::ImageAssets;
use crate::scale::categorical_domain;
use crate::stats;
use crate::theme::Theme;

use super::glyph_plan::PlannedGlyph;
use super::panels::{Panel, PlannedLayer};
use super::row_table::RowSubsetTable;

/// Collect deduplicated fill/stroke/size legends across all spaces (spec §19.5).
pub(super) fn collect_legends(
    panels: &[Panel<'_>],
    theme: &Theme,
    assets: &ImageAssets,
) -> Vec<Legend> {
    // Candidate legends paired with the aesthetic that produced them, so a
    // fill legend and a stroke legend over the same column can be merged below.
    let mut candidates: Vec<(PropertyKey, Legend)> = Vec::new();
    for panel in panels {
        collect_panel_legend_candidates(&mut candidates, panel, theme, assets);
    }
    merge_legends(candidates)
}

fn collect_panel_legend_candidates(
    candidates: &mut Vec<(PropertyKey, Legend)>,
    panel: &Panel<'_>,
    theme: &Theme,
    assets: &ImageAssets,
) {
    if !panel.guides.legend {
        return;
    }
    let rows_table;
    let legend_rows = panel.legend_rows.as_deref();
    let legend_table: &dyn Table = if let Some(rows) = legend_rows {
        rows_table = RowSubsetTable::new(panel.table, rows);
        &rows_table
    } else {
        panel.table
    };
    for layer in &panel.layers {
        match layer {
            PlannedLayer::Geometry(geo) => collect_geometry_legend_candidates(
                candidates,
                panel.frame,
                geo,
                legend_table,
                &panel.scales,
                &panel.guides,
                theme,
                assets,
            ),
            PlannedLayer::Glyph(glyph) => {
                collect_glyph_legend_candidates(candidates, glyph, theme, assets);
            }
        }
    }
}

fn collect_glyph_legend_candidates(
    candidates: &mut Vec<(PropertyKey, Legend)>,
    glyph: &PlannedGlyph<'_>,
    theme: &Theme,
    assets: &ImageAssets,
) {
    if !glyph.legend {
        return;
    }
    if glyph.scale_policy != GlyphScalePolicyIr::Shared {
        return;
    }
    // Child-panel color legends first so the resulting candidate order
    // matches the geometry collector (fill/stroke before size).
    for instance in &glyph.instances {
        for child_panel in &instance.child_panels {
            collect_panel_legend_candidates(candidates, child_panel, theme, assets);
        }
    }
    // A glyph call's `size:` aesthetic (spec §14.27) backed by a glyph-body or
    // chart-scope `Scale(size: col, range:, label:)` produces a size-legend
    // swatch (spec §16.13). Glyph-body wins precedence over chart-scope.
    if let Some(legend) = glyph.size_legend.clone() {
        push_candidate(candidates, PropertyKey::Size, legend);
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_geometry_legend_candidates(
    candidates: &mut Vec<(PropertyKey, Legend)>,
    frame: &FrameIr,
    geo: &GeometryIr,
    table: &dyn Table,
    scales: &[ScaleIr],
    guides: &algraf_semantics::GuideIr,
    theme: &Theme,
    assets: &ImageAssets,
) {
    for aesthetic in [PropertyKey::Fill, PropertyKey::Stroke] {
        if aesthetic == PropertyKey::Fill && !guides.fill_legend {
            continue;
        }
        if aesthetic == PropertyKey::Stroke && !guides.stroke_legend {
            continue;
        }
        if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
            let spec = color_spec(geo, aesthetic, table, scales);
            // A `Scale(<aesthetic>: col, label: "...")` overrides the
            // column-derived legend title (spec §16.13).
            let title = scale_label(scales, aesthetic.as_str())
                .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
            if let Some(legend) = spec.legend(&title) {
                push_candidate(candidates, aesthetic, legend);
            }
        }
    }
    // `HexBin` is rendered by a bespoke geometry rather than desugared to a
    // fill-mapped `Rect` (as `Bin2D` is), so it has no `fill` mapping for
    // the loop above to find. Synthesize the same continuous count legend.
    if geo.kind == GeometryKind::HexBin && guides.fill_legend {
        if let Some(legend) = hexbin_count_legend(geo, frame, table, scales) {
            push_candidate(candidates, PropertyKey::Fill, legend);
        }
    }
    // Size legends for numeric aesthetics: `strokeWidth` and `size`.
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
            scales,
            default_range,
            constant_default,
        );
        let title = scale_label(scales, aesthetic.as_str())
            .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
        if let Some(legend) = spec.legend(&title, kind) {
            push_candidate(candidates, aesthetic, legend);
        }
    }
    if let Some(legend) = shape_legend(geo, table, scales) {
        push_candidate(candidates, PropertyKey::Shape, legend);
    }
    if let Some(legend) = image_legend(geo, table, scales, assets) {
        push_candidate(candidates, PropertyKey::Src, legend);
    }
}

fn push_candidate(
    candidates: &mut Vec<(PropertyKey, Legend)>,
    aesthetic: PropertyKey,
    legend: Legend,
) {
    if !candidates
        .iter()
        .any(|(a, l)| *a == aesthetic && l.title == legend.title)
    {
        candidates.push((aesthetic, legend));
    }
}

/// Build the discrete shape legend for a geometry's `shape` mapping, with one
/// swatch per category in domain order (spec §16.10, §19.5). Entries carry the
/// default mark fill; if the column is also color-mapped the merge step recolors
/// them. Returns `None` when `shape` is unmapped or the domain is empty.
fn shape_legend(geo: &GeometryIr, table: &dyn Table, scales: &[ScaleIr]) -> Option<Legend> {
    let mapping = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Shape)?;
    let categories = categorical_domain(table, &mapping.column.name);
    if categories.is_empty() {
        return None;
    }
    let title = scale_label(scales, "shape")
        .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
    let shapes = (0..categories.len()).map(marker_for_index).collect();
    let entries = categories
        .into_iter()
        .map(|c| (c, DEFAULT_FILL.to_string()))
        .collect();
    Some(Legend {
        title,
        kind: LegendKind::Discrete,
        entries,
        stroke_entries: Vec::new(),
        sizes: Vec::new(),
        shapes,
        images: Vec::new(),
    })
}

/// Build an image legend for mapped `src` values. Constant image sources do not
/// produce a legend, matching point-like constant aesthetics.
fn image_legend(
    geo: &GeometryIr,
    table: &dyn Table,
    scales: &[ScaleIr],
    assets: &ImageAssets,
) -> Option<Legend> {
    if geo.kind != GeometryKind::Image {
        return None;
    }
    let mapping = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Src)?;
    let categories = categorical_domain(table, &mapping.column.name);
    if categories.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    let mut images = Vec::new();
    for category in categories {
        let Some(asset) = assets.get(&category) else {
            continue;
        };
        entries.push((category.clone(), String::new()));
        images.push(LegendImage {
            href: asset.href.clone(),
            intrinsic_width: asset.intrinsic_width,
            intrinsic_height: asset.intrinsic_height,
        });
    }
    if entries.is_empty() {
        return None;
    }
    let title = scale_label(scales, "src")
        .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
    Some(Legend {
        title,
        kind: LegendKind::Image,
        entries,
        stroke_entries: Vec::new(),
        sizes: Vec::new(),
        shapes: Vec::new(),
        images,
    })
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
        breaks: None,
        labels: None,
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

/// Merge legends over the same column into one: a `fill` legend and a `stroke`
/// legend share a title and compatible discrete domains into a single legend
/// whose swatches show both colors (spec §19.7), and a `shape` legend folds its
/// marker glyphs onto a matching color legend so the swatches become those
/// shapes (spec §19.5). Size legends (`Radius`/`Width`) are visually distinct
/// from color legends and pass through as their own legend even when they
/// share a title (spec §16.13).
fn merge_legends(candidates: Vec<(PropertyKey, Legend)>) -> Vec<Legend> {
    let mut out: Vec<Legend> = Vec::new();
    for (aesthetic, legend) in candidates {
        if legend.kind == LegendKind::Image {
            if !out
                .iter()
                .any(|l| l.kind == LegendKind::Image && l.title == legend.title)
            {
                out.push(legend);
            }
            continue;
        }

        // Only the color-legend family (Discrete/Continuous) participates in
        // title-based merging. Size legends (Radius/Width) render circle or
        // line swatches and stay separate even when their title collides with
        // a color legend over the same column.
        let merge_eligible = matches!(legend.kind, LegendKind::Discrete | LegendKind::Continuous);
        let Some(existing) = out.iter_mut().find(|l| {
            l.title == legend.title
                && matches!(l.kind, LegendKind::Discrete | LegendKind::Continuous)
                && merge_eligible
        }) else {
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

        // A shape legend over a color-mapped column keeps the color legend's
        // swatches and labels, but draws each as the mapped marker glyph.
        if aesthetic == PropertyKey::Shape {
            if labels_match && existing.shapes.is_empty() {
                existing.shapes = legend.shapes;
            }
            continue;
        }

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
        // Otherwise the title already has a color legend: keep the first.
    }
    out
}
