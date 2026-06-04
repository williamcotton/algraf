use std::collections::HashMap;

use algraf_semantics::{AestheticMapping, GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_for_row, number_setting};
use crate::guide::estimate_text_width;
use crate::helpers::{bool_setting, string_setting};
use crate::layout::Rect;
use crate::render::TextAnchor;
use crate::scale::{categorical_domain, cell_category, cell_f64, cell_micros};
use crate::sink::{MarkSink, TextRun};
use algraf_data::Table;
use chrono::DateTime;

use super::common::{adjusted_position, any_mapped, pos_center, render_rows};
use super::GeometryRenderContext;

/// A label placed at its (possibly decluttered) screen position.
struct PlacedLabel {
    x: f64,
    y: f64,
    size: f64,
    color: String,
    text: String,
}

/// Render a `Text` geometry: draw labels at each row (spec §14.16).
///
/// `dx`/`dy` may be literals or column mappings (resolved per row). With
/// `declutter: true`, labels that overlap within a shared x column or y row are
/// spread apart before emission (spec §14.16).
pub(super) fn render(sink: &mut dyn MarkSink, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let plot = ctx.plot;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let size = number_setting(geo, PropertyKey::Size, theme.font_size);
    let anchor = match string_setting(geo, PropertyKey::Anchor).as_deref() {
        Some("start") => TextAnchor::Start,
        Some("end") => TextAnchor::End,
        _ => TextAnchor::Middle,
    };
    let declutter = bool_setting(geo, PropertyKey::Declutter, false);

    let label_mapping = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Label);
    let label_literal = string_setting(geo, PropertyKey::Label);
    // An off-axis `timeFormat:` (validated and resolved to a chrono pattern in
    // semantics) formats a temporal `label:` column (spec §19.4).
    let numeric_format = string_setting(geo, PropertyKey::Format);
    let time_format = string_setting(geo, PropertyKey::TimeFormat);
    let literal_positioned_annotation = label_mapping.is_none()
        && label_literal.is_some()
        && geo.mappings.is_empty()
        && geo.settings.iter().any(|s| s.name == PropertyKey::X)
        && geo.settings.iter().any(|s| s.name == PropertyKey::Y);
    let render_row_indices = if literal_positioned_annotation {
        vec![0]
    } else {
        render_rows(table, rows)
    };

    // Phase 1: collect each resolvable label at its post-dx/dy position.
    let mut labels: Vec<PlacedLabel> = Vec::new();
    for row in render_row_indices {
        let x_axis = space.x_axis();
        let y_axis = space.y_axis();
        let cx = if any_mapped(geo, &[PropertyKey::X])
            || geo.settings.iter().any(|s| s.name == PropertyKey::X)
        {
            pos_center(geo, PropertyKey::X, x_axis, table, row)
        } else {
            space.resolve_x(table, row)
        };
        let cy = if any_mapped(geo, &[PropertyKey::Y])
            || geo.settings.iter().any(|s| s.name == PropertyKey::Y)
        {
            y_axis.and_then(|axis| pos_center(geo, PropertyKey::Y, axis, table, row))
        } else {
            space.resolve_y(table, row)
        };
        let (Some(cx), Some(cy)) = (cx, cy) else {
            continue;
        };
        let text = if let Some(mapping) = label_mapping {
            match format_label_cell(
                table,
                &mapping.column.name,
                row,
                numeric_format.as_deref(),
                time_format.as_deref(),
            ) {
                Some(s) => s,
                None => continue,
            }
        } else if let Some(s) = label_literal.clone() {
            s
        } else {
            continue;
        };
        let dx = number_for_row(geo, PropertyKey::Dx, table, row, 0.0);
        let dy = number_for_row(geo, PropertyKey::Dy, table, row, 0.0);
        let (cx, cy) = adjusted_position(geo, space, table, row, cx, cy, false);
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| theme.text_color.clone());
        labels.push(PlacedLabel {
            x: cx + dx,
            y: cy + dy,
            size,
            color,
            text,
        });
    }

    // Phase 2: optionally spread overlapping labels apart.
    if declutter {
        declutter_vertical(&mut labels, plot);
        declutter_horizontal(&mut labels, plot, anchor);
    }

    // Phase 3: emit in collection (row) order for deterministic output.
    for label in &labels {
        emit_label(sink, label, anchor, &theme.font_family, alpha);
    }
}

/// Render a `Label` geometry: draw one terminal label per group at the
/// physical x-axis start or end (spec §14.16).
pub(super) fn render_terminal_label(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let size = number_setting(geo, PropertyKey::Size, theme.font_size);
    let at_start = matches!(
        string_setting(geo, PropertyKey::At).as_deref(),
        Some("start")
    );
    let anchor = match string_setting(geo, PropertyKey::Anchor).as_deref() {
        Some("start") => TextAnchor::Start,
        Some("end") => TextAnchor::End,
        _ => TextAnchor::Middle,
    };
    let numeric_format = string_setting(geo, PropertyKey::Format);
    let label_mapping = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Label);
    let label_literal = string_setting(geo, PropertyKey::Label);
    let row_list = render_rows(table, ctx.rows);

    for group_rows in label_groups(geo, label_mapping, table, row_list) {
        let Some((row, x, y)) = endpoint_row(space, table, &group_rows, at_start) else {
            continue;
        };
        let text = if let Some(mapping) = label_mapping {
            match format_label_cell(
                table,
                &mapping.column.name,
                row,
                numeric_format.as_deref(),
                None,
            ) {
                Some(s) => s,
                None => continue,
            }
        } else if let Some(s) = label_literal.clone() {
            s
        } else {
            continue;
        };
        let dx = number_for_row(geo, PropertyKey::Dx, table, row, 0.0);
        let dy = number_for_row(geo, PropertyKey::Dy, table, row, 0.0);
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| theme.text_color.clone());
        emit_label(
            sink,
            &PlacedLabel {
                x: x + dx,
                y: y + dy,
                size,
                color,
                text,
            },
            anchor,
            &theme.font_family,
            alpha,
        );
    }
}

fn label_groups(
    geo: &GeometryIr,
    label_mapping: Option<&AestheticMapping>,
    table: &dyn Table,
    rows: Vec<usize>,
) -> Vec<Vec<usize>> {
    if let Some(mapping) = geo
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == PropertyKey::Group)
    {
        return rows_by_category(table, &mapping.column.name, &rows);
    }
    if let Some(mapping) = label_mapping {
        return rows_by_category(table, &mapping.column.name, &rows);
    }
    vec![rows]
}

fn rows_by_category(table: &dyn Table, column: &str, rows: &[usize]) -> Vec<Vec<usize>> {
    categorical_domain(table, column)
        .into_iter()
        .map(|category| {
            rows.iter()
                .copied()
                .filter(|&row| {
                    cell_category(table, column, row).as_deref() == Some(category.as_str())
                })
                .collect()
        })
        .collect()
}

fn endpoint_row(
    space: &crate::space::ScaledSpace,
    table: &dyn Table,
    rows: &[usize],
    at_start: bool,
) -> Option<(usize, f64, f64)> {
    let mut endpoint: Option<(usize, f64, f64)> = None;
    for &row in rows {
        let (Some(x), Some(y)) = (space.resolve_x(table, row), space.resolve_y(table, row)) else {
            continue;
        };
        endpoint = match endpoint {
            None => Some((row, x, y)),
            Some((current_row, current_x, current_y)) => {
                let should_replace = if at_start {
                    x < current_x
                } else {
                    x > current_x
                };
                if should_replace {
                    Some((row, x, y))
                } else {
                    Some((current_row, current_x, current_y))
                }
            }
        };
    }
    endpoint
}

/// Resolve a label cell to text. When a chrono `pattern` is supplied and the
/// cell carries a temporal value, format the UTC instant with that pattern;
/// otherwise fall back to the cell's categorical string (spec §19.4).
fn format_label_cell(
    table: &dyn Table,
    column: &str,
    row: usize,
    numeric_format: Option<&str>,
    pattern: Option<&str>,
) -> Option<String> {
    if let Some(format) = numeric_format {
        return cell_f64(table, column, row).map(|value| format_numeric(value, format));
    }
    if let Some(pattern) = pattern {
        if let Some(micros) = cell_micros(table, column, row) {
            return DateTime::from_timestamp_micros(micros)
                .map(|dt| dt.format(pattern).to_string());
        }
    }
    cell_category(table, column, row)
}

fn format_numeric(value: f64, format: &str) -> String {
    match format {
        ".0f" => normalize_negative_zero(format!("{value:.0}")),
        ".1f" => normalize_negative_zero(format!("{value:.1}")),
        ".2f" => normalize_negative_zero(format!("{value:.2}")),
        "$.2f" => {
            let body = normalize_negative_zero(format!("{value:.2}"));
            if let Some(stripped) = body.strip_prefix('-') {
                format!("-${stripped}")
            } else {
                format!("${body}")
            }
        }
        ".0%" => normalize_negative_zero(format!("{:.0}", value * 100.0)) + "%",
        ".1%" => normalize_negative_zero(format!("{:.1}", value * 100.0)) + "%",
        ".2%" => normalize_negative_zero(format!("{:.2}", value * 100.0)) + "%",
        _ => value.to_string(),
    }
}

fn normalize_negative_zero(text: String) -> String {
    if text == "-0" || text.starts_with("-0.") && text[3..].chars().all(|ch| ch == '0') {
        text[1..].to_string()
    } else {
        text
    }
}

fn emit_label(
    sink: &mut dyn MarkSink,
    label: &PlacedLabel,
    anchor: TextAnchor,
    font_family: &str,
    alpha: f64,
) {
    // The sink stacks `\n`-separated content into tspans, matching the SVG
    // backend's multiline behavior (spec §14.16).
    sink.text(&TextRun {
        x: label.x,
        y: label.y,
        anchor,
        rotate: None,
        font_family,
        font_size: label.size,
        fill: &label.color,
        opacity: Some(alpha),
        content: &label.text,
    });
}

/// Spread vertically-overlapping labels apart, grouped by shared x column
/// (spec §14.16). Deterministic: groups by quantized x, and within a group lays
/// labels out with a minimum baseline gap while staying as close as possible to
/// their targets, clamped to the plot's vertical extent.
fn declutter_vertical(labels: &mut [PlacedLabel], plot: Rect) {
    // Group label indices by rounded x so only labels sharing a column interact.
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, label) in labels.iter().enumerate() {
        groups.entry(label.x.round() as i64).or_default().push(i);
    }
    // Deterministic group order.
    let mut keys: Vec<i64> = groups.keys().copied().collect();
    keys.sort_unstable();

    for key in keys {
        let indices = &groups[&key];
        if indices.len() < 2 {
            continue;
        }
        let gap = labels[indices[0]].size * 1.2;
        // Stable target-y sorting, breaking ties by original index.
        let mut order = indices.clone();
        order.sort_by(|&a, &b| labels[a].y.total_cmp(&labels[b].y).then_with(|| a.cmp(&b)));

        let targets: Vec<f64> = order.iter().map(|&i| labels[i].y).collect();
        let mut positions = resolve_min_gap(&targets, gap);
        clamp_group(&mut positions, gap, plot.y, plot.bottom());

        for (k, &i) in order.iter().enumerate() {
            labels[i].y = positions[k];
        }
    }
}

/// Spread horizontally-overlapping labels apart, grouped by shared y row
/// (spec §14.16). Deterministic: groups by quantized baseline y, sorts by the
/// label box center, and lays centers out with estimated text widths.
fn declutter_horizontal(labels: &mut [PlacedLabel], plot: Rect, anchor: TextAnchor) {
    const LABEL_GAP: f64 = 4.0;

    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, label) in labels.iter().enumerate() {
        groups.entry(label.y.round() as i64).or_default().push(i);
    }
    let mut keys: Vec<i64> = groups.keys().copied().collect();
    keys.sort_unstable();

    for key in keys {
        let indices = &groups[&key];
        if indices.len() < 2 {
            continue;
        }

        let mut order = indices.clone();
        order.sort_by(|&a, &b| {
            label_center_x(&labels[a], anchor)
                .total_cmp(&label_center_x(&labels[b], anchor))
                .then_with(|| a.cmp(&b))
        });

        let targets: Vec<f64> = order
            .iter()
            .map(|&i| label_center_x(&labels[i], anchor))
            .collect();
        let widths: Vec<f64> = order.iter().map(|&i| label_width(&labels[i])).collect();
        let gaps: Vec<f64> = widths
            .windows(2)
            .map(|pair| pair[0] / 2.0 + pair[1] / 2.0 + LABEL_GAP)
            .collect();
        let mut centers = resolve_min_gaps(&targets, &gaps);
        clamp_centers(&mut centers, &widths, plot.x, plot.right());

        for (k, &i) in order.iter().enumerate() {
            set_label_center_x(&mut labels[i], anchor, centers[k]);
        }
    }
}

fn label_width(label: &PlacedLabel) -> f64 {
    label
        .text
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            estimate_text_width(line, label.size)
        })
        .fold(0.0, f64::max)
}

fn label_center_x(label: &PlacedLabel, anchor: TextAnchor) -> f64 {
    let width = label_width(label);
    match anchor {
        TextAnchor::Start => label.x + width / 2.0,
        TextAnchor::Middle => label.x,
        TextAnchor::End => label.x - width / 2.0,
    }
}

fn set_label_center_x(label: &mut PlacedLabel, anchor: TextAnchor, center: f64) {
    let width = label_width(label);
    label.x = match anchor {
        TextAnchor::Start => center - width / 2.0,
        TextAnchor::Middle => center,
        TextAnchor::End => center + width / 2.0,
    };
}

/// Lay out ascending `targets` so adjacent positions are at least `gap` apart,
/// minimizing displacement (a 1-D isotonic / pool-adjacent-violators layout).
/// Returns positions aligned with `targets`. Deterministic and O(n).
fn resolve_min_gap(targets: &[f64], gap: f64) -> Vec<f64> {
    let gaps = vec![gap; targets.len().saturating_sub(1)];
    resolve_min_gaps(targets, &gaps)
}

/// Lay out ascending `targets` so adjacent positions are at least each
/// corresponding `gaps[i]` apart, minimizing displacement. Deterministic and
/// O(n), using isotonic regression after subtracting cumulative gaps.
fn resolve_min_gaps(targets: &[f64], gaps: &[f64]) -> Vec<f64> {
    const EPS: f64 = 1e-9;
    debug_assert_eq!(gaps.len(), targets.len().saturating_sub(1));
    if targets.is_empty() {
        return Vec::new();
    }

    let mut offsets = Vec::with_capacity(targets.len());
    offsets.push(0.0);
    for &gap in gaps {
        let previous = *offsets.last().unwrap_or(&0.0);
        offsets.push(previous + gap.max(0.0));
    }

    // Subtract cumulative required gaps, solve nondecreasing isotonic
    // regression, then add the gaps back.
    struct Cluster {
        count: usize,
        sum: f64,
    }
    let mut clusters: Vec<Cluster> = Vec::with_capacity(targets.len());
    for (&target, &offset) in targets.iter().zip(&offsets) {
        clusters.push(Cluster {
            count: 1,
            sum: target - offset,
        });
        while clusters.len() >= 2 {
            let b = &clusters[clusters.len() - 1];
            let a = &clusters[clusters.len() - 2];
            let a_mean = a.sum / a.count as f64;
            let b_mean = b.sum / b.count as f64;
            if a_mean > b_mean + EPS {
                let merged = Cluster {
                    count: a.count + b.count,
                    sum: a.sum + b.sum,
                };
                clusters.pop();
                clusters.pop();
                clusters.push(merged);
            } else {
                break;
            }
        }
    }

    let mut positions = Vec::with_capacity(targets.len());
    for c in &clusters {
        let mean = c.sum / c.count as f64;
        for _ in 0..c.count {
            let offset = offsets[positions.len()];
            positions.push(mean + offset);
        }
    }
    positions
}

/// Shift a laid-out group into `[top + gap, bottom]`. Prefers fitting the top;
/// if the group is taller than the band it overflows downward rather than
/// crushing the spacing.
fn clamp_group(positions: &mut [f64], gap: f64, top: f64, bottom: f64) {
    let top_limit = top + gap;
    let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
        return;
    };
    if first < top_limit {
        let shift = top_limit - first;
        positions.iter_mut().for_each(|p| *p += shift);
    } else if last > bottom {
        // Shift up to fit the bottom, but never push the top above its limit.
        let shift = (last - bottom).min(first - top_limit);
        if shift > 0.0 {
            positions.iter_mut().for_each(|p| *p -= shift);
        }
    }
}

/// Shift center positions into `[left, right]` when the group fits. If the
/// estimated label boxes are wider than the plot, preserve spacing and overflow
/// to the right, matching the vertical declutter behavior.
fn clamp_centers(positions: &mut [f64], widths: &[f64], left: f64, right: f64) {
    let (Some(&first), Some(&last), Some(&first_width), Some(&last_width)) = (
        positions.first(),
        positions.last(),
        widths.first(),
        widths.last(),
    ) else {
        return;
    };
    let first_left = first - first_width / 2.0;
    let last_right = last + last_width / 2.0;
    if first_left < left {
        let shift = left - first_left;
        positions.iter_mut().for_each(|p| *p += shift);
    } else if last_right > right {
        let shift = (last_right - right).min(first_left - left);
        if shift > 0.0 {
            positions.iter_mut().for_each(|p| *p -= shift);
        }
    }
}
