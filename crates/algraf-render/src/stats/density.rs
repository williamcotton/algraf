use algraf_data::{Column, ColumnView, DataFrame, DataType, Table};

use crate::scale::cell_f64;

use super::util::{col_def, deterministic_frame};

/// Options for kernel density estimation.
#[derive(Debug, Clone, Copy)]
pub struct DensityOptions {
    /// Explicit kernel bandwidth, or `None` to use Silverman's rule of thumb.
    pub bandwidth: Option<f64>,
    /// Number of evaluation points across the grid.
    pub grid_points: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityPoint {
    pub x: f64,
    pub density: f64,
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
    let value_view = table.column(input_column);
    let mut values: Vec<f64> = (0..table.row_count())
        .filter_map(|row| f64_cell(value_view, table, input_column, row))
        .filter(|v| v.is_finite())
        .collect();

    let schema = vec![
        col_def("density_x", DataType::Float),
        col_def("density", DataType::Float),
    ];
    let points = density_values(&mut values, options);
    if points.is_empty() {
        return deterministic_frame(
            schema,
            vec![
                Column::from_float_options(vec![]),
                Column::from_float_options(vec![]),
            ],
        );
    }

    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(points.iter().map(|point| Some(point.x)).collect()),
            Column::from_float_options(points.iter().map(|point| Some(point.density)).collect()),
        ],
    )
}

/// Compute blended kernel density estimates over several numeric columns.
/// Each column is estimated independently, and all outputs are concatenated
/// into a shared derived table with a `series` column indicating the source.
pub fn density_blended(
    table: &dyn Table,
    value_columns: &[&str],
    options: DensityOptions,
) -> DataFrame {
    let schema = vec![
        col_def("density_x", DataType::Float),
        col_def("density", DataType::Float),
        col_def("series", DataType::String),
    ];

    let mut xs = Vec::new();
    let mut ds = Vec::new();
    let mut series = Vec::new();

    for column in value_columns {
        let value_view = table.column(column);
        let mut values: Vec<f64> = (0..table.row_count())
            .filter_map(|row| f64_cell(value_view, table, column, row))
            .filter(|v| v.is_finite())
            .collect();
        let points = density_values(&mut values, options);
        for p in points {
            xs.push(Some(p.x));
            ds.push(Some(p.density));
            series.push(Some(column.to_string()));
        }
    }

    if xs.is_empty() {
        return deterministic_frame(
            schema,
            vec![
                Column::from_float_options(vec![]),
                Column::from_float_options(vec![]),
                Column::String(vec![]),
            ],
        );
    }

    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(xs),
            Column::from_float_options(ds),
            Column::String(series),
        ],
    )
}

fn f64_cell(
    view: Option<ColumnView<'_>>,
    table: &dyn Table,
    column: &str,
    row: usize,
) -> Option<f64> {
    match view {
        Some(view) => view.f64_at(row),
        None => cell_f64(table, column, row),
    }
}

pub fn density_values(values: &mut [f64], options: DensityOptions) -> Vec<DensityPoint> {
    values.sort_by(f64::total_cmp);
    if values.len() < 2 {
        return Vec::new();
    }
    let n = values.len() as f64;
    let bandwidth = options
        .bandwidth
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| silverman_bandwidth(values));
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
    let mut points = Vec::with_capacity(grid_points);
    for i in 0..grid_points {
        let x = lo + step * i as f64;
        let sum: f64 = values
            .iter()
            .map(|v| gaussian_kernel((x - v) / bandwidth))
            .sum();
        points.push(DensityPoint {
            x,
            density: sum * inv,
        });
    }
    points
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
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
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

#[cfg(test)]
mod density_tests {
    use super::*;

    fn frame_of(values: &[f64]) -> DataFrame {
        DataFrame::new(
            vec![col_def("v", DataType::Float)],
            vec![Column::from_float_options(
                values.iter().map(|v| Some(*v)).collect(),
            )],
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
