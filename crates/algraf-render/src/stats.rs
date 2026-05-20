//! Statistical transforms for derived tables (spec §15).
//!
//! Version 0.1 implements the `Bin` stat, producing `bin_start`, `bin_end`,
//! `bin_center`, and `count` columns (spec §15.6).

use algraf_data::{Column, ColumnDef, DataFrame, DataType, Table};

use crate::scale::{cell_f64, numeric_domain};

/// Options for numeric histogram binning.
#[derive(Debug, Clone, Copy)]
pub struct BinOptions {
    pub bins: usize,
    pub bin_width: Option<f64>,
    pub boundary: Option<f64>,
}

/// Compute a histogram-bin derived table over a numeric input column.
pub fn bin_with_options(table: &dyn Table, input_column: &str, options: BinOptions) -> DataFrame {
    let bins = options.bins.max(1);
    let (min, max) = numeric_domain(table, input_column).unwrap_or((0.0, 1.0));
    let (start, width, bin_count) = bin_layout(min, max, bins, options);

    let mut counts = vec![0i64; bin_count];
    for row in 0..table.row_count() {
        if let Some(v) = cell_f64(table, input_column, row) {
            let mut idx = ((v - start) / width).floor() as isize;
            idx = idx.clamp(0, bin_count as isize - 1);
            counts[idx as usize] += 1;
        }
    }

    let mut starts = Vec::with_capacity(bin_count);
    let mut ends = Vec::with_capacity(bin_count);
    let mut centers = Vec::with_capacity(bin_count);
    for i in 0..bin_count {
        let bin_start = start + i as f64 * width;
        let bin_end = bin_start + width;
        starts.push(Some(bin_start));
        ends.push(Some(bin_end));
        centers.push(Some((bin_start + bin_end) / 2.0));
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

fn bin_layout(min: f64, max: f64, bins: usize, options: BinOptions) -> (f64, f64, usize) {
    if let Some(bin_width) = options.bin_width {
        if bin_width.is_finite() && bin_width > f64::EPSILON {
            let boundary = options.boundary.unwrap_or(0.0);
            let start = boundary + ((min - boundary) / bin_width).floor() * bin_width;
            let mut end = boundary + ((max - boundary) / bin_width).ceil() * bin_width;
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

fn col_def(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}
