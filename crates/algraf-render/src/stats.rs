//! Statistical transforms for derived tables (spec §15).
//!
//! Version 0.1 implements the `Bin` stat, producing `bin_start`, `bin_end`,
//! `bin_center`, and `count` columns (spec §15.6).

use algraf_data::{Column, ColumnDef, DataFrame, DataType, Table};

use crate::scale::{cell_f64, numeric_domain};

/// Compute a histogram-bin derived table over a numeric input column.
pub fn bin(table: &dyn Table, input_column: &str, bins: usize) -> DataFrame {
    let bins = bins.max(1);
    let (min, max) = numeric_domain(table, input_column).unwrap_or((0.0, 1.0));
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let width = span / bins as f64;

    let mut counts = vec![0i64; bins];
    for row in 0..table.row_count() {
        if let Some(v) = cell_f64(table, input_column, row) {
            let mut idx = ((v - min) / width).floor() as isize;
            idx = idx.clamp(0, bins as isize - 1);
            counts[idx as usize] += 1;
        }
    }

    let mut starts = Vec::with_capacity(bins);
    let mut ends = Vec::with_capacity(bins);
    let mut centers = Vec::with_capacity(bins);
    for i in 0..bins {
        let start = min + i as f64 * width;
        let end = start + width;
        starts.push(Some(start));
        ends.push(Some(end));
        centers.push(Some((start + end) / 2.0));
    }

    let schema = vec![
        col_def("bin_start", DataType::Float),
        col_def("bin_end", DataType::Float),
        col_def("bin_center", DataType::Float),
        col_def("count", DataType::Integer),
    ];
    let columns = vec![
        Column::Float(starts),
        Column::Float(ends),
        Column::Float(centers),
        Column::Int(counts.into_iter().map(Some).collect()),
    ];
    DataFrame::new(schema, columns)
}

fn col_def(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}
