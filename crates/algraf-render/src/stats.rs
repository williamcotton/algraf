//! Statistical transforms for derived tables (spec §15).
//!
//! Version 0.1 implements the `Bin` stat (spec §15.6), producing `bin_start`,
//! `bin_end`, `bin_center`, `count`, and `density` columns, and the `Count`
//! stat (spec §15.5), producing one row per category with a `count` column.

use algraf_data::{Column, ColumnDef, DataFrame, DataType, Table};

use crate::scale::{categorical_domain, cell_category, cell_f64, numeric_domain};

/// Options for numeric histogram binning.
#[derive(Debug, Clone, Copy)]
pub struct BinOptions {
    pub bins: usize,
    pub bin_width: Option<f64>,
    pub boundary: Option<f64>,
    pub closed: BinClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinClosed {
    Left,
    Right,
}

/// Compute a histogram-bin derived table over a numeric input column.
pub fn bin_with_options(table: &dyn Table, input_column: &str, options: BinOptions) -> DataFrame {
    let bins = options.bins.max(1);
    let (min, max) = numeric_domain(table, input_column).unwrap_or((0.0, 1.0));
    let (start, width, bin_count) = bin_layout(min, max, bins, options);

    let mut counts = vec![0i64; bin_count];
    let mut total_count: i64 = 0;
    for row in 0..table.row_count() {
        if let Some(v) = cell_f64(table, input_column, row) {
            let idx = bin_index(v, start, width, bin_count, options.closed);
            counts[idx] += 1;
            total_count += 1;
        }
    }

    let mut starts = Vec::with_capacity(bin_count);
    let mut ends = Vec::with_capacity(bin_count);
    let mut centers = Vec::with_capacity(bin_count);
    let mut densities = Vec::with_capacity(bin_count);
    let total = total_count as f64;
    for (i, &count) in counts.iter().enumerate() {
        let bin_start = start + i as f64 * width;
        let bin_end = bin_start + width;
        starts.push(Some(bin_start));
        ends.push(Some(bin_end));
        centers.push(Some((bin_start + bin_end) / 2.0));
        // Density = count / (total * width), so densities integrate to 1.
        let density = if total > 0.0 && width.abs() > f64::EPSILON {
            count as f64 / (total * width)
        } else {
            0.0
        };
        densities.push(Some(density));
    }

    let schema = vec![
        col_def("bin_start", DataType::Float),
        col_def("bin_end", DataType::Float),
        col_def("bin_center", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ];
    let columns = vec![
        Column::Float(starts),
        Column::Float(ends),
        Column::Float(centers),
        Column::Int(counts.into_iter().map(Some).collect()),
        Column::Float(densities),
    ];
    DataFrame::new(schema, columns)
}

/// Compute a count derived table grouping rows by one or two categorical
/// columns (spec §15.5). Output columns are the group keys (preserving input
/// type) followed by an integer `count` column.
pub fn count_by(table: &dyn Table, group_columns: &[&str]) -> DataFrame {
    assert!(
        !group_columns.is_empty(),
        "count_by requires a group column"
    );
    let outer = group_columns[0];
    let inner = group_columns.get(1).copied();
    let outer_cats = categorical_domain(table, outer);

    let mut rows: Vec<(String, Option<String>, i64)> = Vec::new();
    if let Some(inner_col) = inner {
        let inner_cats = categorical_domain(table, inner_col);
        for o in &outer_cats {
            for i in &inner_cats {
                let count: i64 = (0..table.row_count())
                    .filter(|&row| {
                        cell_category(table, outer, row).as_deref() == Some(o.as_str())
                            && cell_category(table, inner_col, row).as_deref() == Some(i.as_str())
                    })
                    .count() as i64;
                rows.push((o.clone(), Some(i.clone()), count));
            }
        }
    } else {
        for o in &outer_cats {
            let count: i64 = (0..table.row_count())
                .filter(|&row| cell_category(table, outer, row).as_deref() == Some(o.as_str()))
                .count() as i64;
            rows.push((o.clone(), None, count));
        }
    }

    let mut schema = vec![col_def(outer, DataType::String)];
    let outer_col = Column::String(rows.iter().map(|r| Some(r.0.clone())).collect());
    let mut columns = vec![outer_col];
    if let Some(inner_col) = inner {
        schema.push(col_def(inner_col, DataType::String));
        columns.push(Column::String(rows.iter().map(|r| r.1.clone()).collect()));
    }
    schema.push(col_def("count", DataType::Integer));
    columns.push(Column::Int(rows.iter().map(|r| Some(r.2)).collect()));
    DataFrame::new(schema, columns)
}

fn bin_layout(min: f64, max: f64, bins: usize, options: BinOptions) -> (f64, f64, usize) {
    if let Some(bin_width) = options.bin_width {
        if bin_width.is_finite() && bin_width > f64::EPSILON {
            let boundary = options.boundary.unwrap_or(0.0);
            let min_offset = (min - boundary) / bin_width;
            let max_offset = (max - boundary) / bin_width;
            let start_index = match options.closed {
                BinClosed::Left => min_offset.floor(),
                BinClosed::Right if is_integerish(min_offset) => min_offset.floor() - 1.0,
                BinClosed::Right => min_offset.floor(),
            };
            let mut end_index = max_offset.ceil();
            if options.closed == BinClosed::Left && is_integerish(max_offset) {
                end_index += 1.0;
            }
            let start = boundary + start_index * bin_width;
            let mut end = boundary + end_index * bin_width;
            if end <= start {
                end = start + bin_width;
            }
            let bin_count = ((end - start) / bin_width).round().max(1.0) as usize;
            return (start, bin_width, bin_count);
        }
    }

    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let width = span / bins as f64;
    (min, width, bins)
}

fn bin_index(value: f64, start: f64, width: f64, bin_count: usize, closed: BinClosed) -> usize {
    let raw = (value - start) / width;
    let idx = match closed {
        BinClosed::Left => raw.floor(),
        BinClosed::Right => raw.ceil() - 1.0,
    } as isize;
    idx.clamp(0, bin_count as isize - 1) as usize
}

fn is_integerish(value: f64) -> bool {
    (value - value.round()).abs() <= 1e-10
}

fn col_def(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}
