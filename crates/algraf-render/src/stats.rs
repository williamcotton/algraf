//! Statistical transforms for derived tables (spec §15).
//!
//! Version 0.1 implements the `Bin` stat (spec §15.6), producing `bin_start`,
//! `bin_end`, `bin_center`, `count`, and `density` columns, and the `Count`
//! stat (spec §15.5), producing one row per category with a `count` column.

use algraf_data::{Column, ColumnDef, DataFrame, DataType, DateTimeValue, Table};
use chrono::DateTime;

use crate::scale::{
    categorical_domain, cell_category, cell_f64, cell_micros, numeric_domain, temporal_domain,
};

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
    if table
        .schema()
        .iter()
        .any(|column| column.name == input_column && column.dtype == DataType::Temporal)
    {
        return temporal_bin_with_options(table, input_column, options);
    }

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

/// Compute a histogram-bin derived table over a temporal input column.
pub fn temporal_bin_with_options(
    table: &dyn Table,
    input_column: &str,
    options: BinOptions,
) -> DataFrame {
    let bins = options.bins.max(1);
    let (min, max, precision) = temporal_domain(table, input_column).unwrap_or((
        0,
        1,
        algraf_data::TemporalPrecision::Date,
    ));
    let numeric_options = BinOptions {
        bins,
        bin_width: options.bin_width,
        boundary: options.boundary,
        closed: options.closed,
    };
    let (start, width, bin_count) = bin_layout(min as f64, max as f64, bins, numeric_options);

    let mut counts = vec![0i64; bin_count];
    let mut total_count: i64 = 0;
    for row in 0..table.row_count() {
        if let Some(v) = cell_micros(table, input_column, row) {
            let idx = bin_index(v as f64, start, width, bin_count, options.closed);
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
        let bin_center = (bin_start + bin_end) / 2.0;
        starts.push(datetime_value(bin_start.round() as i64, precision));
        ends.push(datetime_value(bin_end.round() as i64, precision));
        centers.push(datetime_value(bin_center.round() as i64, precision));
        let density = if total > 0.0 && width.abs() > f64::EPSILON {
            count as f64 / (total * width.abs())
        } else {
            0.0
        };
        densities.push(Some(density));
    }

    let schema = vec![
        col_def("bin_start", DataType::Temporal),
        col_def("bin_end", DataType::Temporal),
        col_def("bin_center", DataType::Temporal),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ];
    let columns = vec![
        Column::Temporal(starts),
        Column::Temporal(ends),
        Column::Temporal(centers),
        Column::Int(counts.into_iter().map(Some).collect()),
        Column::Float(densities),
    ];
    DataFrame::new(schema, columns)
}

/// Options for kernel density estimation.
#[derive(Debug, Clone, Copy)]
pub struct DensityOptions {
    /// Explicit kernel bandwidth, or `None` to use Silverman's rule of thumb.
    pub bandwidth: Option<f64>,
    /// Number of evaluation points across the grid.
    pub grid_points: usize,
}

impl Default for DensityOptions {
    fn default() -> Self {
        DensityOptions {
            bandwidth: None,
            grid_points: 256,
        }
    }
}

/// Compute a Gaussian kernel-density estimate over a numeric input column
/// (spec §15.11). The output has `density_x` and `density` columns; the curve
/// integrates to approximately 1.
///
/// Defaults are deterministic: a Gaussian kernel, Silverman's rule-of-thumb
/// bandwidth `0.9 * min(stddev, IQR/1.349) * n^(-1/5)`, a 256-point grid, and a
/// 3-bandwidth extension past the data range.
pub fn density(table: &dyn Table, input_column: &str, options: DensityOptions) -> DataFrame {
    let mut values: Vec<f64> = (0..table.row_count())
        .filter_map(|row| cell_f64(table, input_column, row))
        .filter(|v| v.is_finite())
        .collect();
    values.sort_by(f64::total_cmp);

    let schema = vec![
        col_def("density_x", DataType::Float),
        col_def("density", DataType::Float),
    ];
    if values.len() < 2 {
        return DataFrame::new(schema, vec![Column::Float(vec![]), Column::Float(vec![])]);
    }

    let n = values.len() as f64;
    let bandwidth = options
        .bandwidth
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| silverman_bandwidth(&values));
    // Degenerate spread: every value equal. Emit a single spike-free flat curve.
    let bandwidth = if bandwidth > f64::EPSILON {
        bandwidth
    } else {
        1.0
    };

    let grid_points = options.grid_points.max(2);
    let cut = 3.0 * bandwidth;
    let lo = values[0] - cut;
    let hi = values[values.len() - 1] + cut;
    let step = (hi - lo) / (grid_points - 1) as f64;

    let inv = 1.0 / (n * bandwidth);
    let mut xs = Vec::with_capacity(grid_points);
    let mut ds = Vec::with_capacity(grid_points);
    for i in 0..grid_points {
        let x = lo + step * i as f64;
        let sum: f64 = values
            .iter()
            .map(|v| gaussian_kernel((x - v) / bandwidth))
            .sum();
        xs.push(Some(x));
        ds.push(Some(sum * inv));
    }

    DataFrame::new(schema, vec![Column::Float(xs), Column::Float(ds)])
}

/// Silverman's rule-of-thumb bandwidth. `values` must be sorted ascending.
fn silverman_bandwidth(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    let iqr = percentile(values, 0.75) - percentile(values, 0.25);
    let spread = if iqr > 0.0 {
        std_dev.min(iqr / 1.349)
    } else {
        std_dev
    };
    0.9 * spread * n.powf(-0.2)
}

/// Linear-interpolated percentile of a sorted slice.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

fn gaussian_kernel(u: f64) -> f64 {
    const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
    INV_SQRT_2PI * (-0.5 * u * u).exp()
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

fn datetime_value(micros: i64, precision: algraf_data::TemporalPrecision) -> Option<DateTimeValue> {
    DateTime::from_timestamp_micros(micros)
        .map(|instant| DateTimeValue::new(instant.naive_utc(), precision))
}

fn col_def(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}

#[cfg(test)]
mod density_tests {
    use super::*;

    fn frame_of(values: &[f64]) -> DataFrame {
        DataFrame::new(
            vec![col_def("v", DataType::Float)],
            vec![Column::Float(values.iter().map(|v| Some(*v)).collect())],
        )
    }

    #[test]
    fn density_curve_integrates_to_about_one() {
        // A Gaussian KDE is a proper density: it integrates to ~1 (spec §15.11).
        let values: Vec<f64> = (0..50).map(|i| (i as f64) * 0.1).collect();
        let df = frame_of(&values);
        let out = density(&df, "v", DensityOptions::default());
        let xs: Vec<f64> = (0..out.row_count())
            .filter_map(|r| cell_f64(&out, "density_x", r))
            .collect();
        let ds: Vec<f64> = (0..out.row_count())
            .filter_map(|r| cell_f64(&out, "density", r))
            .collect();
        assert_eq!(xs.len(), ds.len());
        assert!(xs.len() >= 2);
        // Trapezoidal integration over the grid.
        let mut area = 0.0;
        for i in 1..xs.len() {
            area += (xs[i] - xs[i - 1]) * (ds[i] + ds[i - 1]) / 2.0;
        }
        assert!(
            (area - 1.0).abs() < 0.05,
            "density should integrate to ~1, got {area}"
        );
        // All density values are non-negative.
        assert!(ds.iter().all(|d| *d >= 0.0));
    }

    #[test]
    fn density_respects_grid_point_count() {
        let df = frame_of(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let out = density(
            &df,
            "v",
            DensityOptions {
                bandwidth: None,
                grid_points: 64,
            },
        );
        assert_eq!(out.row_count(), 64);
    }

    #[test]
    fn density_handles_fewer_than_two_points() {
        let df = frame_of(&[1.0]);
        let out = density(&df, "v", DensityOptions::default());
        assert_eq!(out.row_count(), 0);
    }
}
