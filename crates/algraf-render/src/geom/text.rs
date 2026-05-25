use std::collections::HashMap;

use algraf_semantics::GeometryIr;

use crate::aes::{color_spec, number_for_row, number_setting};
use crate::helpers::{bool_setting, string_setting};
use crate::layout::Rect;
use crate::scale::cell_category;
use crate::svg::{escape_attr, escape_text, num, SvgWriter};

use super::common::render_rows;
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
/// `declutter: true`, labels that overlap vertically within a shared x column
/// are spread apart before emission (spec §14.16).
pub(super) fn render(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let plot = ctx.plot;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.font_size);
    let anchor = string_setting(geo, "anchor").unwrap_or_else(|| "middle".to_string());
    let declutter = bool_setting(geo, "declutter", false);

    let label_mapping = geo.mappings.iter().find(|m| m.aesthetic == "label");
    let label_literal = string_setting(geo, "label");

    // Phase 1: collect each resolvable label at its post-dx/dy position.
    let mut labels: Vec<PlacedLabel> = Vec::new();
    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let text = if let Some(mapping) = label_mapping {
            match cell_category(table, &mapping.column.name, row) {
                Some(s) => s,
                None => continue,
            }
        } else if let Some(s) = label_literal.clone() {
            s
        } else {
            continue;
        };
        let dx = number_for_row(geo, "dx", table, row, 0.0);
        let dy = number_for_row(geo, "dy", table, row, 0.0);
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

    // Phase 2: optionally spread vertically-overlapping labels apart.
    if declutter {
        declutter_vertical(&mut labels, plot);
    }

    // Phase 3: emit in collection (row) order for deterministic output.
    for label in &labels {
        w.line(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\">{}</text>",
            num(label.x),
            num(label.y),
            escape_attr(&anchor),
            escape_attr(&theme.font_family),
            num(label.size),
            escape_attr(&label.color),
            num(alpha),
            escape_text(&label.text),
        ));
    }
}

/// Spread labels that overlap vertically apart, grouped by shared x column
/// (spec §14.16). Deterministic: groups by quantized x, and within a group lays
/// labels out with a minimum gap while staying as close as possible to their
/// targets, clamped to the plot's vertical extent.
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
        // Stable order by target y, breaking ties by original index.
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

/// Lay out ascending `targets` so adjacent positions are at least `gap` apart,
/// minimizing displacement (a 1-D isotonic / pool-adjacent-violators layout).
/// Returns positions aligned with `targets`. Deterministic and O(n).
fn resolve_min_gap(targets: &[f64], gap: f64) -> Vec<f64> {
    const EPS: f64 = 1e-9;
    // Each cluster lays its members out at `gap` spacing centered on the mean of
    // its members' targets. Merge adjacent clusters that would overlap.
    struct Cluster {
        count: usize,
        sum: f64,
    }
    let mut clusters: Vec<Cluster> = Vec::with_capacity(targets.len());
    for &t in targets {
        clusters.push(Cluster { count: 1, sum: t });
        while clusters.len() >= 2 {
            let b = &clusters[clusters.len() - 1];
            let a = &clusters[clusters.len() - 2];
            let a_mean = a.sum / a.count as f64;
            let b_mean = b.sum / b.count as f64;
            let a_bottom = a_mean + (a.count as f64 - 1.0) / 2.0 * gap;
            let b_top = b_mean - (b.count as f64 - 1.0) / 2.0 * gap;
            if b_top - a_bottom < gap - EPS {
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
        let first = mean - (c.count as f64 - 1.0) / 2.0 * gap;
        for j in 0..c.count {
            positions.push(first + j as f64 * gap);
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
