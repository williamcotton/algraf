//! Visual stack-order hints for stacked legends (spec §14.6, §14.14, §19.5).
//!
//! Stack accumulation order is a geometry-placement detail; legend order is a
//! guide-presentation detail. For stacked layouts the default legend lists
//! entries in the order a reader scans the rendered stack: the positive side
//! outward-to-baseline (so a vertical stack's top band, or a rightward stack's
//! rightmost band, comes first), then the negative side baseline-outward. When
//! one categorical domain holds several disjoint visible stack cohorts, each
//! cohort is ordered internally by that rule while the cohorts themselves keep
//! the scale/domain order of their earliest category. Only visible (nonzero)
//! stack contributions participate; zero-height placeholder cells for sparse
//! stacked areas do not link cohorts. Reordering is presentation-only: it never
//! changes scale domains, color assignment, or geometry placement.

use std::collections::{HashMap, HashSet};

use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, PropertyKey, ScaleIr};

use crate::aes::{color_spec, ColorSpec};
use crate::helpers::{area_layout, bar_layout, AreaLayout, BarLayout};
use crate::scale::{cell_category, cell_f64};
use crate::space::ScaledSpace;

use super::common::{
    categorical_value_orientation, position_group_key, render_rows, value_axis_data_column,
};

/// A visible stack contribution: the category's index in the legend domain and
/// whether it accumulates on the positive side of the baseline.
type StackEntry = (usize, bool);

/// The default legend display order for a stacked geometry's categorical color
/// aesthetic, as indices into `categories`. Returns `None` when the geometry is
/// not a visibly stacked layout for this aesthetic, leaving the scale/domain
/// order untouched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stacked_legend_order(
    geo: &GeometryIr,
    aesthetic: PropertyKey,
    categories: &[String],
    column: &str,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) -> Option<Vec<usize>> {
    if categories.is_empty() {
        return None;
    }
    let sequences = match geo.kind {
        GeometryKind::Bar => bar_sequences(geo, categories, column, space, table, rows)?,
        GeometryKind::Area => area_sequences(
            geo, aesthetic, categories, column, space, table, rows, scales,
        )?,
        // A grouped stacked Histogram desugars to pre-stacked `Rect` rows with
        // `stack_lower`/`stack_upper` bounds (spec §14.7).
        GeometryKind::Rect => prestacked_rect_sequences(geo, categories, column, table, rows)?,
        _ => return None,
    };
    Some(display_order(categories.len(), &sequences))
}

/// Per-position accumulation sequences for a stacked Bar. Segments accumulate
/// in row order at each position key (spec §14.6).
fn bar_sequences(
    geo: &GeometryIr,
    categories: &[String],
    column: &str,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
) -> Option<Vec<Vec<StackEntry>>> {
    if !matches!(bar_layout(geo), BarLayout::Stack | BarLayout::Fill) || space.is_polar() {
        return None;
    }
    let orientation = categorical_value_orientation(space)?;
    let value_col = value_axis_data_column(space, orientation)?.to_string();
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut sequences: Vec<Vec<StackEntry>> = Vec::new();
    for row in render_rows(table, rows) {
        let Some(value) = cell_f64(table, &value_col, row) else {
            continue;
        };
        if value.abs() <= f64::EPSILON {
            continue;
        }
        let Some(index) = category_index(categories, table, column, row) else {
            continue;
        };
        let key = position_group_key(space, table, row, orientation).unwrap_or_default();
        let next = sequences.len();
        let slot = *positions.entry(key).or_insert(next);
        if slot == sequences.len() {
            sequences.push(Vec::new());
        }
        let sequence = &mut sequences[slot];
        if !sequence.iter().any(|(i, _)| *i == index) {
            sequence.push((index, value >= 0.0));
        }
    }
    Some(sequences)
}

/// Per-x accumulation sequences for a stacked Area. Groups accumulate in the
/// grouping aesthetic's domain order at each physical x position, positive and
/// negative values separately (spec §14.14). Applies only when `aesthetic` is
/// the aesthetic that actually forms the stack groups.
#[allow(clippy::too_many_arguments)]
fn area_sequences(
    geo: &GeometryIr,
    aesthetic: PropertyKey,
    categories: &[String],
    column: &str,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) -> Option<Vec<Vec<StackEntry>>> {
    if !matches!(area_layout(geo), AreaLayout::Stack | AreaLayout::Fill) || space.is_polar() {
        return None;
    }
    // Mirror the grouping precedence of `render_area`: an explicit `group`
    // mapping forms the stack (its bands need not align with this aesthetic's
    // categories), otherwise categorical/binned `fill` wins over `stroke`.
    if geo
        .mappings
        .iter()
        .any(|mapping| mapping.aesthetic == PropertyKey::Group)
    {
        return None;
    }
    if aesthetic == PropertyKey::Stroke
        && matches!(
            color_spec(geo, PropertyKey::Fill, table, scales),
            ColorSpec::Categorical { .. } | ColorSpec::Binned { .. }
        )
    {
        return None;
    }
    let value_col = space
        .y_axis()
        .and_then(|axis| axis.data_column())?
        .to_string();

    // Aggregate each (category, x) cell exactly as stacking does, then walk the
    // x positions in pixel order emitting each position's visible groups in
    // domain (accumulation) order.
    let mut x_positions: Vec<(u64, f64)> = Vec::new();
    let mut cell_values: HashMap<(usize, u64), f64> = HashMap::new();
    for row in render_rows(table, rows) {
        let Some(x) = space.resolve_x(table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, &value_col, row) else {
            continue;
        };
        let Some(index) = category_index(categories, table, column, row) else {
            continue;
        };
        let key = x.to_bits();
        x_positions.push((key, x));
        *cell_values.entry((index, key)).or_insert(0.0) += value;
    }
    x_positions.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    x_positions.dedup_by(|a, b| a.0 == b.0);

    let sequences = x_positions
        .iter()
        .map(|&(key, _)| {
            (0..categories.len())
                .filter_map(|index| {
                    let value = *cell_values.get(&(index, key))?;
                    // Zero-height cells (including the placeholder cells sparse
                    // stacked areas evaluate) are invisible and must not link
                    // cohorts.
                    (value.abs() > f64::EPSILON).then_some((index, value >= 0.0))
                })
                .collect()
        })
        .collect();
    Some(sequences)
}

/// Per-bin accumulation sequences for pre-stacked `Rect` rows carrying
/// `stack_lower`/`stack_upper` bounds — the grouped stacked Histogram
/// desugaring (spec §14.7). Rows are ordered baseline-outward within each bin
/// by their near-baseline stack bound.
fn prestacked_rect_sequences(
    geo: &GeometryIr,
    categories: &[String],
    column: &str,
    table: &dyn Table,
    rows: Option<&[usize]>,
) -> Option<Vec<Vec<StackEntry>>> {
    let mapped = |aesthetic: PropertyKey| {
        geo.mappings
            .iter()
            .find(|mapping| mapping.aesthetic == aesthetic)
            .map(|mapping| mapping.column.name.as_str())
    };
    // Vertical bins stack along y; horizontal bins stack along x.
    let bin_col = if mapped(PropertyKey::Ymin) == Some("stack_lower")
        && mapped(PropertyKey::Ymax) == Some("stack_upper")
    {
        mapped(PropertyKey::Xmin)?
    } else if mapped(PropertyKey::Xmin) == Some("stack_lower")
        && mapped(PropertyKey::Xmax) == Some("stack_upper")
    {
        mapped(PropertyKey::Ymin)?
    } else {
        return None;
    };

    let mut positions: HashMap<u64, usize> = HashMap::new();
    let mut bins: Vec<Vec<(f64, usize, bool)>> = Vec::new();
    for row in render_rows(table, rows) {
        let (Some(lower), Some(upper)) = (
            cell_f64(table, "stack_lower", row),
            cell_f64(table, "stack_upper", row),
        ) else {
            continue;
        };
        if (upper - lower).abs() <= f64::EPSILON {
            continue;
        }
        let Some(bin) = cell_f64(table, bin_col, row) else {
            continue;
        };
        let Some(index) = category_index(categories, table, column, row) else {
            continue;
        };
        let next = bins.len();
        let slot = *positions.entry(bin.to_bits()).or_insert(next);
        if slot == bins.len() {
            bins.push(Vec::new());
        }
        // Distance of the segment's near edge from the baseline orders the
        // bin's segments baseline-outward on either side.
        let near = lower.abs().min(upper.abs());
        bins[slot].push((near, index, lower + upper >= 0.0));
    }
    let sequences = bins
        .into_iter()
        .map(|mut bin| {
            bin.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut sequence: Vec<StackEntry> = Vec::new();
            for (_, index, positive) in bin {
                if !sequence.iter().any(|(i, _)| *i == index) {
                    sequence.push((index, positive));
                }
            }
            sequence
        })
        .collect();
    Some(sequences)
}

fn category_index(
    categories: &[String],
    table: &dyn Table,
    column: &str,
    row: usize,
) -> Option<usize> {
    let category = cell_category(table, column, row)?;
    categories.iter().position(|c| *c == category)
}

/// Merge per-position accumulation sequences into the legend display order.
///
/// Categories that visibly stack together at any position form a cohort.
/// Within a cohort the accumulation order is reconstructed from consecutive
/// pairs in the sequences (first-seen direction wins on conflict; ties and
/// cycle breaks fall back to domain order), then the positive side is listed
/// outward-to-baseline (reverse accumulation) followed by the negative side
/// baseline-outward. A category's side is the side of its first visible
/// contribution. Cohorts — including categories with no visible stack
/// contribution, which form singleton cohorts — are ordered by their earliest
/// member in domain order.
fn display_order(domain_len: usize, sequences: &[Vec<StackEntry>]) -> Vec<usize> {
    // Union-find over co-stacking categories.
    let mut parent: Vec<usize> = (0..domain_len).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut positive: Vec<Option<bool>> = vec![None; domain_len];
    let mut contributes = vec![false; domain_len];
    let mut edges: HashSet<(usize, usize)> = HashSet::new();
    for sequence in sequences {
        for &(index, sign) in sequence {
            contributes[index] = true;
            positive[index].get_or_insert(sign);
        }
        for pair in sequence.windows(2) {
            let (a, b) = (pair[0].0, pair[1].0);
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
            if a != b && !edges.contains(&(b, a)) {
                edges.insert((a, b));
            }
        }
    }

    // Accumulation order: Kahn's algorithm over contributing categories,
    // preferring the smallest domain index among ready nodes; a remaining
    // conflict cycle breaks at the smallest domain index.
    let mut indegree = vec![0usize; domain_len];
    for &(_, b) in &edges {
        indegree[b] += 1;
    }
    let mut emitted = vec![false; domain_len];
    let mut accumulation_rank = vec![0usize; domain_len];
    let pending = contributes.iter().filter(|&&c| c).count();
    for rank in 0..pending {
        let ready = (0..domain_len)
            .filter(|&i| contributes[i] && !emitted[i])
            .find(|&i| indegree[i] == 0);
        let Some(next) = ready.or_else(|| (0..domain_len).find(|&i| contributes[i] && !emitted[i]))
        else {
            break;
        };
        emitted[next] = true;
        accumulation_rank[next] = rank;
        for &(a, b) in &edges {
            if a == next && !emitted[b] {
                indegree[b] -= 1;
            }
        }
    }

    // Cohorts in order of their earliest member in domain order; categories
    // without visible contributions stay singletons at their domain position.
    let mut cohort_members: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut cohort_slot: HashMap<usize, usize> = HashMap::new();
    for (index, &contributing) in contributes.iter().enumerate() {
        let root = if contributing {
            find(&mut parent, index)
        } else {
            index
        };
        match cohort_slot.get(&root) {
            Some(&slot) => cohort_members[slot].1.push(index),
            None => {
                cohort_slot.insert(root, cohort_members.len());
                cohort_members.push((index, vec![index]));
            }
        }
    }
    cohort_members.sort_by_key(|(earliest, _)| *earliest);

    let mut order = Vec::with_capacity(domain_len);
    for (_, members) in cohort_members {
        let mut positives: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| positive[i] != Some(false))
            .collect();
        positives.sort_by_key(|&i| accumulation_rank[i]);
        positives.reverse();
        let mut negatives: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| positive[i] == Some(false))
            .collect();
        negatives.sort_by_key(|&i| accumulation_rank[i]);
        order.extend(positives);
        order.extend(negatives);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::display_order;

    #[test]
    fn single_cohort_positive_stack_reverses_accumulation() {
        // One stack: deletions accumulate first, additions on top. The legend
        // reads top-to-bottom: additions first.
        let sequences = vec![vec![(0, true), (1, true)], vec![(0, true), (1, true)]];
        assert_eq!(display_order(2, &sequences), vec![1, 0]);
    }

    #[test]
    fn disjoint_cohorts_keep_domain_cohort_order() {
        // Domain: [before-del, before-add, after-del, after-add]. The cohorts
        // never co-stack, so a full-domain reverse would be wrong.
        let sequences = vec![vec![(0, true), (1, true)], vec![(2, true), (3, true)]];
        assert_eq!(display_order(4, &sequences), vec![1, 0, 3, 2]);
    }

    #[test]
    fn negative_side_follows_positive_side_baseline_outward() {
        // Positive: a then b (visual top = b). Negative: c then d outward.
        // Reading order: b, a, then c, d.
        let sequences = vec![vec![(0, true), (1, true), (2, false), (3, false)]];
        assert_eq!(display_order(4, &sequences), vec![1, 0, 2, 3]);
    }

    #[test]
    fn non_contributing_categories_keep_domain_position() {
        // Category 1 never stacks visibly; it stays between the cohorts that
        // surround it in the domain.
        let sequences = vec![vec![(2, true), (3, true)]];
        assert_eq!(display_order(4, &sequences), vec![0, 1, 3, 2]);
    }

    #[test]
    fn conflicting_pair_directions_resolve_first_seen_then_domain_order() {
        // Two positions disagree about 0 vs 1; the first-seen direction (0
        // before 1) wins, so the display reverses it deterministically.
        let sequences = vec![vec![(0, true), (1, true)], vec![(1, true), (0, true)]];
        assert_eq!(display_order(2, &sequences), vec![1, 0]);
    }

    #[test]
    fn no_visible_contributions_keep_domain_order() {
        assert_eq!(display_order(3, &[]), vec![0, 1, 2]);
    }
}
