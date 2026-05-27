//! Statistical transforms for derived tables (spec §15).
//!
//! Version 0.1 implements the `Bin` stat (spec §15.6), producing `bin_start`,
//! `bin_end`, `bin_center`, `count`, and `density` columns, and the `Count`
//! stat (spec §15.5), producing one row per category with a `count` column.

use algraf_data::{Column, ColumnDef, DataFrame, DataType, DateTimeValue, Table};
use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, Timelike};

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
    pub interval: Option<BinInterval>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinClosed {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinInterval {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

/// Compute a grouped histogram-bin derived table over a numeric value column,
/// split by a categorical group column (spec §15.6). All groups share the same
/// bin edges (computed over the global value domain) so bars align. Counts are
/// pre-stacked per bin in group order: each emitted row carries `stack_lower`
/// and `stack_upper`, the cumulative y-bounds for a stacked bar. Output columns
/// are `bin_start`, `bin_end`, `bin_center`, `count`, `density`, the group
/// column (preserving its name), `stack_lower`, and `stack_upper` (cumulative
/// y-bounds for a stacked bar), plus `dodge_start`/`dodge_end` (the per-group
/// side-by-side sub-slot within the bin, for a dodged bar). Rows are emitted
/// bin-major, group-minor, for deterministic order (spec §18.12).
pub fn bin_grouped(
    table: &dyn Table,
    value_column: &str,
    group_column: &str,
    options: BinOptions,
) -> DataFrame {
    let bins = options.bins.max(1);
    let (min, max) = numeric_domain(table, value_column).unwrap_or((0.0, 1.0));
    let (start, width, bin_count) = bin_layout(min, max, bins, options);
    let groups = categorical_domain(table, group_column);

    // counts[bin * groups.len() + group_index]
    let mut counts = vec![0i64; bin_count * groups.len().max(1)];
    let mut total: i64 = 0;
    for row in 0..table.row_count() {
        let (Some(v), Some(g)) = (
            cell_f64(table, value_column, row),
            cell_category(table, group_column, row),
        ) else {
            continue;
        };
        let Some(gi) = groups.iter().position(|c| c == &g) else {
            continue;
        };
        let bi = bin_index(v, start, width, bin_count, options.closed);
        counts[bi * groups.len() + gi] += 1;
        total += 1;
    }

    let total_f = total as f64;
    let group_count = groups.len().max(1) as f64;
    let mut bin_starts = Vec::new();
    let mut bin_ends = Vec::new();
    let mut bin_centers = Vec::new();
    let mut row_counts = Vec::new();
    let mut densities = Vec::new();
    let mut group_keys = Vec::new();
    let mut stack_lowers = Vec::new();
    let mut stack_uppers = Vec::new();
    let mut dodge_starts = Vec::new();
    let mut dodge_ends = Vec::new();
    for bi in 0..bin_count {
        let bin_start = start + bi as f64 * width;
        let bin_end = bin_start + width;
        let mut cumulative = 0i64;
        for (gi, group) in groups.iter().enumerate() {
            let count = counts[bi * groups.len() + gi];
            let lower = cumulative;
            cumulative += count;
            bin_starts.push(Some(bin_start));
            bin_ends.push(Some(bin_end));
            bin_centers.push(Some((bin_start + bin_end) / 2.0));
            row_counts.push(Some(count));
            let density = if total_f > 0.0 && width.abs() > f64::EPSILON {
                count as f64 / (total_f * width.abs())
            } else {
                0.0
            };
            densities.push(Some(density));
            group_keys.push(Some(group.clone()));
            stack_lowers.push(Some(lower as f64));
            stack_uppers.push(Some(cumulative as f64));
            // Side-by-side sub-slot: split the bin into one slot per group, in
            // group order, for an algebraically-dodged histogram (spec §15.6).
            dodge_starts.push(Some(bin_start + width * gi as f64 / group_count));
            dodge_ends.push(Some(bin_start + width * (gi as f64 + 1.0) / group_count));
        }
    }

    let schema = vec![
        col_def("bin_start", DataType::Float),
        col_def("bin_end", DataType::Float),
        col_def("bin_center", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
        col_def(group_column, DataType::String),
        col_def("stack_lower", DataType::Float),
        col_def("stack_upper", DataType::Float),
        col_def("dodge_start", DataType::Float),
        col_def("dodge_end", DataType::Float),
    ];
    DataFrame::new(
        schema,
        vec![
            Column::Float(bin_starts),
            Column::Float(bin_ends),
            Column::Float(bin_centers),
            Column::Int(row_counts),
            Column::Float(densities),
            Column::String(group_keys),
            Column::Float(stack_lowers),
            Column::Float(stack_uppers),
            Column::Float(dodge_starts),
            Column::Float(dodge_ends),
        ],
    )
}

/// Compute a blended histogram-bin derived table over several numeric columns
/// (spec §15.6). All series share bin edges computed from the combined domain,
/// and each output row carries a synthetic `series` value naming the source
/// column. Rows are emitted bin-major, series-minor for deterministic output.
pub fn bin_blended(table: &dyn Table, value_columns: &[&str], options: BinOptions) -> DataFrame {
    let bins = options.bins.max(1);
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for column in value_columns {
        if let Some((lo, hi)) = numeric_domain(table, column) {
            min = min.min(lo);
            max = max.max(hi);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        min = 0.0;
        max = 1.0;
    }
    let (start, width, bin_count) = bin_layout(min, max, bins, options);

    let series_count = value_columns.len();
    let mut counts = vec![0i64; bin_count * series_count.max(1)];
    let mut totals = vec![0i64; series_count];
    for row in 0..table.row_count() {
        for (si, column) in value_columns.iter().enumerate() {
            if let Some(v) = cell_f64(table, column, row) {
                let bi = bin_index(v, start, width, bin_count, options.closed);
                counts[bi * series_count + si] += 1;
                totals[si] += 1;
            }
        }
    }

    let mut bin_starts = Vec::new();
    let mut bin_ends = Vec::new();
    let mut bin_centers = Vec::new();
    let mut row_counts = Vec::new();
    let mut densities = Vec::new();
    let mut series = Vec::new();
    for bi in 0..bin_count {
        let bin_start = start + bi as f64 * width;
        let bin_end = bin_start + width;
        for (si, column) in value_columns.iter().enumerate() {
            let count = counts[bi * series_count + si];
            bin_starts.push(Some(bin_start));
            bin_ends.push(Some(bin_end));
            bin_centers.push(Some((bin_start + bin_end) / 2.0));
            row_counts.push(Some(count));
            let total = totals[si] as f64;
            let density = if total > 0.0 && width.abs() > f64::EPSILON {
                count as f64 / (total * width.abs())
            } else {
                0.0
            };
            densities.push(Some(density));
            series.push(Some((*column).to_string()));
        }
    }

    let schema = vec![
        col_def("bin_start", DataType::Float),
        col_def("bin_end", DataType::Float),
        col_def("bin_center", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
        col_def("series", DataType::String),
    ];
    DataFrame::new(
        schema,
        vec![
            Column::Float(bin_starts),
            Column::Float(bin_ends),
            Column::Float(bin_centers),
            Column::Int(row_counts),
            Column::Float(densities),
            Column::String(series),
        ],
    )
}

/// Compute a histogram-bin derived table over a numeric input column.
pub fn bin_with_options(table: &dyn Table, input_column: &str, options: BinOptions) -> DataFrame {
    if table
        .schema()
        .iter()
        .any(|column| column.name == input_column && column.dtype == DataType::Temporal)
    {
        if let Some(interval) = options.interval {
            return temporal_calendar_bin(table, input_column, options, interval);
        }
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
        interval: None,
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

fn temporal_calendar_bin(
    table: &dyn Table,
    input_column: &str,
    options: BinOptions,
    interval: BinInterval,
) -> DataFrame {
    let Some((min, max, precision)) = temporal_domain(table, input_column) else {
        return empty_temporal_bin_frame();
    };
    let Some(min_dt) = DateTime::from_timestamp_micros(min).map(|dt| dt.naive_utc()) else {
        return empty_temporal_bin_frame();
    };
    let Some(max_dt) = DateTime::from_timestamp_micros(max).map(|dt| dt.naive_utc()) else {
        return empty_temporal_bin_frame();
    };
    let Some(first_start) = floor_interval(min_dt, interval) else {
        return empty_temporal_bin_frame();
    };
    let mut last_start = floor_interval(max_dt, interval).unwrap_or(first_start);
    if options.closed == BinClosed::Right && max_dt == last_start && max_dt > first_start {
        last_start = previous_interval(last_start, interval).unwrap_or(last_start);
    }

    let mut starts = Vec::new();
    let mut cursor = first_start;
    let mut guard = 0usize;
    while cursor <= last_start && guard < 20_000 {
        starts.push(cursor);
        let Some(next) = add_interval(cursor, interval) else {
            break;
        };
        if next <= cursor {
            break;
        }
        cursor = next;
        guard += 1;
    }
    if starts.is_empty() {
        starts.push(first_start);
    }

    let mut counts = vec![0i64; starts.len()];
    let mut total_count = 0i64;
    for row in 0..table.row_count() {
        let Some(micros) = cell_micros(table, input_column, row) else {
            continue;
        };
        let Some(dt) = DateTime::from_timestamp_micros(micros).map(|dt| dt.naive_utc()) else {
            continue;
        };
        let mut start = floor_interval(dt, interval).unwrap_or(first_start);
        if options.closed == BinClosed::Right && dt == start && dt > first_start {
            start = previous_interval(start, interval).unwrap_or(start);
        }
        if let Some(index) = starts.iter().position(|candidate| *candidate == start) {
            counts[index] += 1;
            total_count += 1;
        }
    }

    let mut out_starts = Vec::with_capacity(starts.len());
    let mut out_ends = Vec::with_capacity(starts.len());
    let mut centers = Vec::with_capacity(starts.len());
    let mut densities = Vec::with_capacity(starts.len());
    let total = total_count as f64;
    for (index, start) in starts.iter().copied().enumerate() {
        let end = add_interval(start, interval).unwrap_or(start);
        out_starts.push(Some(DateTimeValue::new(start, precision)));
        out_ends.push(Some(DateTimeValue::new(end, precision)));
        centers.push(midpoint_temporal(start, end, precision));
        let width = end
            .and_utc()
            .timestamp_micros()
            .saturating_sub(start.and_utc().timestamp_micros())
            .abs() as f64;
        let density = if total > 0.0 && width > f64::EPSILON {
            counts[index] as f64 / (total * width)
        } else {
            0.0
        };
        densities.push(Some(density));
    }

    let columns = vec![
        Column::Temporal(out_starts),
        Column::Temporal(out_ends),
        Column::Temporal(centers),
        Column::Int(counts.into_iter().map(Some).collect()),
        Column::Float(densities),
    ];
    DataFrame::new(temporal_bin_schema(), columns)
}

fn empty_temporal_bin_frame() -> DataFrame {
    DataFrame::new(
        temporal_bin_schema(),
        vec![
            Column::Temporal(vec![]),
            Column::Temporal(vec![]),
            Column::Temporal(vec![]),
            Column::Int(vec![]),
            Column::Float(vec![]),
        ],
    )
}

fn temporal_bin_schema() -> Vec<ColumnDef> {
    vec![
        col_def("bin_start", DataType::Temporal),
        col_def("bin_end", DataType::Temporal),
        col_def("bin_center", DataType::Temporal),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ]
}

fn floor_interval(dt: NaiveDateTime, interval: BinInterval) -> Option<NaiveDateTime> {
    let date = dt.date();
    match interval {
        BinInterval::Minute => date.and_hms_opt(dt.hour(), dt.minute(), 0),
        BinInterval::Hour => date.and_hms_opt(dt.hour(), 0, 0),
        BinInterval::Day => date.and_hms_opt(0, 0, 0),
        BinInterval::Week => {
            let days = date.weekday().num_days_from_monday() as i64;
            date.checked_sub_signed(Duration::days(days))?
                .and_hms_opt(0, 0, 0)
        }
        BinInterval::Month => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1)?.and_hms_opt(0, 0, 0)
        }
        BinInterval::Quarter => {
            let month = ((date.month() - 1) / 3) * 3 + 1;
            NaiveDate::from_ymd_opt(date.year(), month, 1)?.and_hms_opt(0, 0, 0)
        }
        BinInterval::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1)?.and_hms_opt(0, 0, 0),
    }
}

fn add_interval(dt: NaiveDateTime, interval: BinInterval) -> Option<NaiveDateTime> {
    match interval {
        BinInterval::Minute => dt.checked_add_signed(Duration::minutes(1)),
        BinInterval::Hour => dt.checked_add_signed(Duration::hours(1)),
        BinInterval::Day => dt.checked_add_signed(Duration::days(1)),
        BinInterval::Week => dt.checked_add_signed(Duration::weeks(1)),
        BinInterval::Month => dt.checked_add_months(Months::new(1)),
        BinInterval::Quarter => dt.checked_add_months(Months::new(3)),
        BinInterval::Year => dt.checked_add_months(Months::new(12)),
    }
}

fn previous_interval(dt: NaiveDateTime, interval: BinInterval) -> Option<NaiveDateTime> {
    match interval {
        BinInterval::Minute => dt.checked_sub_signed(Duration::minutes(1)),
        BinInterval::Hour => dt.checked_sub_signed(Duration::hours(1)),
        BinInterval::Day => dt.checked_sub_signed(Duration::days(1)),
        BinInterval::Week => dt.checked_sub_signed(Duration::weeks(1)),
        BinInterval::Month => dt.checked_sub_months(Months::new(1)),
        BinInterval::Quarter => dt.checked_sub_months(Months::new(3)),
        BinInterval::Year => dt.checked_sub_months(Months::new(12)),
    }
}

fn midpoint_temporal(
    start: NaiveDateTime,
    end: NaiveDateTime,
    precision: algraf_data::TemporalPrecision,
) -> Option<DateTimeValue> {
    match precision {
        algraf_data::TemporalPrecision::Date => {
            let days = end
                .date()
                .signed_duration_since(start.date())
                .num_days()
                .max(0);
            let date = start.date().checked_add_signed(Duration::days(days / 2))?;
            Some(DateTimeValue::new(
                date.and_hms_opt(0, 0, 0)?,
                algraf_data::TemporalPrecision::Date,
            ))
        }
        algraf_data::TemporalPrecision::DateTime => {
            let a = start.and_utc().timestamp_micros();
            let b = end.and_utc().timestamp_micros();
            datetime_value(a + (b - a) / 2, precision)
        }
    }
}

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
    let mut values: Vec<f64> = (0..table.row_count())
        .filter_map(|row| cell_f64(table, input_column, row))
        .filter(|v| v.is_finite())
        .collect();

    let schema = vec![
        col_def("density_x", DataType::Float),
        col_def("density", DataType::Float),
    ];
    let points = density_values(&mut values, options);
    if points.is_empty() {
        return DataFrame::new(schema, vec![Column::Float(vec![]), Column::Float(vec![])]);
    }

    DataFrame::new(
        schema,
        vec![
            Column::Float(points.iter().map(|point| Some(point.x)).collect()),
            Column::Float(points.iter().map(|point| Some(point.density)).collect()),
        ],
    )
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

#[derive(Debug, Clone, Copy)]
pub struct Bin2DOptions {
    pub bins: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexBinCell {
    pub x: f64,
    pub y: f64,
    /// Horizontal half-extent of the bin, in x-data units.
    pub radius: f64,
    /// Vertical half-extent of the bin, in y-data units. Carried separately
    /// from `radius` because x and y live on independent scales: reusing the
    /// x-unit `radius` for the y axis collapses every hexagon into a sliver.
    pub y_radius: f64,
    pub count: i64,
    pub density: f64,
}

/// Compute rectangular 2D bins for two numeric columns.
pub fn bin2d(table: &dyn Table, x_col: &str, y_col: &str, options: Bin2DOptions) -> DataFrame {
    let cells = bin2d_cells(table, x_col, y_col, options);
    let schema = vec![
        col_def("x_start", DataType::Float),
        col_def("x_end", DataType::Float),
        col_def("x_center", DataType::Float),
        col_def("y_start", DataType::Float),
        col_def("y_end", DataType::Float),
        col_def("y_center", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ];
    DataFrame::new(
        schema,
        vec![
            Column::Float(cells.iter().map(|c| Some(c.0)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.1)).collect()),
            Column::Float(cells.iter().map(|c| Some((c.0 + c.1) / 2.0)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.2)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.3)).collect()),
            Column::Float(cells.iter().map(|c| Some((c.2 + c.3) / 2.0)).collect()),
            Column::Int(cells.iter().map(|c| Some(c.4)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.5)).collect()),
        ],
    )
}

fn bin2d_cells(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: Bin2DOptions,
) -> Vec<(f64, f64, f64, f64, i64, f64)> {
    let bins = options.bins.max(1);
    let (x_min, x_max) = numeric_domain(table, x_col).unwrap_or((0.0, 1.0));
    let (y_min, y_max) = numeric_domain(table, y_col).unwrap_or((0.0, 1.0));
    let (x_start, x_width, x_bins) = bin_layout(
        x_min,
        x_max,
        bins,
        BinOptions {
            bins,
            bin_width: None,
            boundary: None,
            closed: BinClosed::Left,
            interval: None,
        },
    );
    let (y_start, y_width, y_bins) = bin_layout(
        y_min,
        y_max,
        bins,
        BinOptions {
            bins,
            bin_width: None,
            boundary: None,
            closed: BinClosed::Left,
            interval: None,
        },
    );
    let mut counts = vec![0i64; x_bins * y_bins];
    let mut total = 0i64;
    for row in 0..table.row_count() {
        let (Some(x), Some(y)) = (cell_f64(table, x_col, row), cell_f64(table, y_col, row)) else {
            continue;
        };
        let xi = bin_index(x, x_start, x_width, x_bins, BinClosed::Left);
        let yi = bin_index(y, y_start, y_width, y_bins, BinClosed::Left);
        counts[yi * x_bins + xi] += 1;
        total += 1;
    }
    let area = (x_width * y_width).abs();
    let denom = total as f64 * area;
    let mut cells = Vec::new();
    for yi in 0..y_bins {
        for xi in 0..x_bins {
            let count = counts[yi * x_bins + xi];
            if count == 0 {
                continue;
            }
            let xs = x_start + xi as f64 * x_width;
            let xe = xs + x_width;
            let ys = y_start + yi as f64 * y_width;
            let ye = ys + y_width;
            let density = if denom > f64::EPSILON {
                count as f64 / denom
            } else {
                0.0
            };
            cells.push((xs, xe, ys, ye, count, density));
        }
    }
    cells
}

/// Compute deterministic hexagonal bins for two numeric columns.
///
/// Binning happens in normalized `[0, 1]` space so that the hexagon lattice
/// tessellates regardless of the x/y data ranges: a regular hexagon honeycomb
/// in normalized coordinates maps, under the (independent, linear) x and y
/// scales, to a gap-free tiling of stretched hexagons in pixel space. Each
/// observation is assigned to its *nearest* lattice center (the standard
/// pointy-top, odd-row-offset scheme), so occupied neighbors share edges.
pub fn hexbin(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: Bin2DOptions,
) -> Vec<HexBinCell> {
    let bins = options.bins.max(1);
    let (x_min, x_max) = numeric_domain(table, x_col).unwrap_or((0.0, 1.0));
    let (y_min, y_max) = numeric_domain(table, y_col).unwrap_or((0.0, 1.0));
    let x_span = if (x_max - x_min).abs() < f64::EPSILON {
        1.0
    } else {
        x_max - x_min
    };
    let y_span = if (y_max - y_min).abs() < f64::EPSILON {
        1.0
    } else {
        y_max - y_min
    };

    // Hexagon radius (center-to-vertex) in normalized units, sized so roughly
    // `bins` columns span the unit width. For a pointy-top lattice the column
    // spacing is `dx = r*sqrt(3)` and the row spacing is `dy = r*1.5`.
    let r = 1.0 / (bins as f64 * 3.0_f64.sqrt());
    let dx = r * 3.0_f64.sqrt();
    let dy = r * 1.5;

    // Accumulate counts keyed by lattice center. `(2*pi, pj)` is an integer
    // key (pi is a multiple of 0.5), and a BTreeMap keeps emission order
    // deterministic (spec §18.12).
    let mut counts: std::collections::BTreeMap<(i64, i64), i64> = std::collections::BTreeMap::new();
    let mut total = 0i64;
    for row in 0..table.row_count() {
        let (Some(x), Some(y)) = (cell_f64(table, x_col, row), cell_f64(table, y_col, row)) else {
            continue;
        };
        let u = (x - x_min) / x_span;
        let v = (y - y_min) / y_span;
        let (pi, pj) = hex_lattice_index(u, v, dx, dy);
        let key = ((pi * 2.0).round() as i64, pj as i64);
        *counts.entry(key).or_insert(0) += 1;
        total += 1;
    }

    // Hex area in data units, using the (stretched) x and y radii.
    let rx_data = r * x_span.abs();
    let ry_data = r * y_span.abs();
    let hex_area = 3.0 * 3.0_f64.sqrt() * rx_data * ry_data / 2.0;
    let denom = total as f64 * hex_area;

    counts
        .into_iter()
        .map(|((pi2, pj), count)| {
            let pi = pi2 as f64 / 2.0;
            let pj = pj as f64;
            let u = pi * dx;
            let v = pj * dy;
            let x = x_min + u * x_span;
            let y = y_min + v * y_span;
            let density = if denom > f64::EPSILON {
                count as f64 / denom
            } else {
                0.0
            };
            HexBinCell {
                x,
                y,
                radius: rx_data,
                y_radius: ry_data,
                count,
                density,
            }
        })
        .collect()
}

/// Assign a normalized point `(u, v)` to the nearest pointy-top hex-lattice
/// center, returning the center as `(pi, pj)` where the center is at
/// `(pi*dx, pj*dy)`. `pi` is a multiple of `0.5` (odd rows are offset by half a
/// column). Mirrors the d3-hexbin assignment.
fn hex_lattice_index(u: f64, v: f64, dx: f64, dy: f64) -> (f64, f64) {
    let py = v / dy;
    let pj = py.round();
    let px = u / dx
        - if (pj as i64).rem_euclid(2) == 1 {
            0.5
        } else {
            0.0
        };
    let pi = px.round();
    let py1 = py - pj;
    if py1.abs() * 3.0 > 1.0 {
        let px1 = px - pi;
        let pi2 = pi + if px < pi { -0.5 } else { 0.5 };
        let pj2 = pj + if py < pj { -1.0 } else { 1.0 };
        let px2 = px - pi2;
        let py2 = py - pj2;
        if px1 * px1 + py1 * py1 > px2 * px2 + py2 * py2 {
            let pi = pi2
                + if (pj as i64).rem_euclid(2) == 1 {
                    0.5
                } else {
                    -0.5
                };
            return (
                pi + if (pj2 as i64).rem_euclid(2) == 1 {
                    0.5
                } else {
                    0.0
                },
                pj2,
            );
        }
    }
    (
        pi + if (pj as i64).rem_euclid(2) == 1 {
            0.5
        } else {
            0.0
        },
        pj,
    )
}

pub fn hexbin_frame(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: Bin2DOptions,
) -> DataFrame {
    let cells = hexbin(table, x_col, y_col, options);
    let schema = vec![
        col_def("x", DataType::Float),
        col_def("y", DataType::Float),
        col_def("radius", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ];
    DataFrame::new(
        schema,
        vec![
            Column::Float(cells.iter().map(|c| Some(c.x)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.y)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.radius)).collect()),
            Column::Int(cells.iter().map(|c| Some(c.count)).collect()),
            Column::Float(cells.iter().map(|c| Some(c.density)).collect()),
        ],
    )
}

/// The smoothing method for [`smooth_points`] (spec §15.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothMethod {
    /// Ordinary least-squares linear fit.
    Lm,
    /// Locally weighted (tricube) degree-1 regression.
    Loess,
}

/// Options for [`smooth_points`].
#[derive(Debug, Clone, Copy)]
pub struct SmoothOptions {
    pub method: SmoothMethod,
    /// Loess neighborhood fraction in `(0, 1]`.
    pub span: f64,
    /// Emit confidence-band half-widths in [`SmoothPoint::se`].
    pub se: bool,
    /// Number of evaluation points across the x-range when a curve is sampled
    /// (loess, or `lm` with `se`). A plain `lm` line uses two endpoints.
    pub eval_points: usize,
    /// Multiplier applied to the standard error to form the band half-width
    /// (1.96 ≈ 95% under a normal approximation, spec §15.x).
    pub z: f64,
}

impl Default for SmoothOptions {
    fn default() -> Self {
        SmoothOptions {
            method: SmoothMethod::Lm,
            span: 0.75,
            se: false,
            eval_points: 80,
            z: 1.96,
        }
    }
}

/// One sampled point on a fitted smooth, with the standard error of the fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothPoint {
    pub x: f64,
    pub y: f64,
    /// Standard error of the fitted `y` (0 when `se` is disabled).
    pub se: f64,
}

/// Materialize a `Smooth` derived table over two numeric columns (spec §15.x).
/// Columns are always `x`, `y`; when `options.se` is set, `ymin`, `ymax`, and
/// `se` confidence-band columns follow.
pub fn smooth(table: &dyn Table, x_col: &str, y_col: &str, options: SmoothOptions) -> DataFrame {
    let mut points: Vec<(f64, f64)> = (0..table.row_count())
        .filter_map(|row| Some((cell_f64(table, x_col, row)?, cell_f64(table, y_col, row)?)))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    let fitted = smooth_points(&mut points, options);

    if options.se {
        let schema = vec![
            col_def("x", DataType::Float),
            col_def("y", DataType::Float),
            col_def("ymin", DataType::Float),
            col_def("ymax", DataType::Float),
            col_def("se", DataType::Float),
        ];
        return DataFrame::new(
            schema,
            vec![
                Column::Float(fitted.iter().map(|p| Some(p.x)).collect()),
                Column::Float(fitted.iter().map(|p| Some(p.y)).collect()),
                Column::Float(
                    fitted
                        .iter()
                        .map(|p| Some(p.y - options.z * p.se))
                        .collect(),
                ),
                Column::Float(
                    fitted
                        .iter()
                        .map(|p| Some(p.y + options.z * p.se))
                        .collect(),
                ),
                Column::Float(fitted.iter().map(|p| Some(p.se)).collect()),
            ],
        );
    }

    let schema = vec![col_def("x", DataType::Float), col_def("y", DataType::Float)];
    DataFrame::new(
        schema,
        vec![
            Column::Float(fitted.iter().map(|p| Some(p.x)).collect()),
            Column::Float(fitted.iter().map(|p| Some(p.y)).collect()),
        ],
    )
}

/// Fit a smooth over `(x, y)` points and sample it deterministically.
///
/// `points` is sorted in place by x. Returns an empty vector when there are too
/// few distinct x values to fit a line. For `lm` without `se`, the result is the
/// two line endpoints; otherwise the curve is sampled at `eval_points` evenly
/// spaced x values across the observed range.
pub fn smooth_points(points: &mut [(f64, f64)], options: SmoothOptions) -> Vec<SmoothPoint> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let x_min = points[0].0;
    let x_max = points[n - 1].0;
    if (x_max - x_min).abs() <= f64::EPSILON {
        return Vec::new();
    }

    match options.method {
        SmoothMethod::Lm => lm_points(points, options, x_min, x_max),
        SmoothMethod::Loess => loess_points(points, options, x_min, x_max),
    }
}

fn eval_grid(x_min: f64, x_max: f64, count: usize) -> Vec<f64> {
    let count = count.max(2);
    let step = (x_max - x_min) / (count - 1) as f64;
    (0..count).map(|i| x_min + step * i as f64).collect()
}

fn lm_points(
    points: &[(f64, f64)],
    options: SmoothOptions,
    x_min: f64,
    x_max: f64,
) -> Vec<SmoothPoint> {
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| *x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| *y).sum::<f64>() / n;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in points {
        sxx += (x - mean_x).powi(2);
        sxy += (x - mean_x) * (y - mean_y);
    }
    if sxx.abs() <= f64::EPSILON {
        return Vec::new();
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    let predict = |x: f64| intercept + slope * x;

    if !options.se {
        return vec![
            SmoothPoint {
                x: x_min,
                y: predict(x_min),
                se: 0.0,
            },
            SmoothPoint {
                x: x_max,
                y: predict(x_max),
                se: 0.0,
            },
        ];
    }

    // Residual standard deviation of the fit (RSS / (n - 2)); the standard error
    // of the fitted mean at x is s * sqrt(1/n + (x - x̄)² / Sxx).
    let rss: f64 = points.iter().map(|(x, y)| (y - predict(*x)).powi(2)).sum();
    let dof = (points.len() as f64 - 2.0).max(1.0);
    let s = (rss / dof).sqrt();
    eval_grid(x_min, x_max, options.eval_points)
        .into_iter()
        .map(|x| {
            let se = s * (1.0 / n + (x - mean_x).powi(2) / sxx).sqrt();
            SmoothPoint {
                x,
                y: predict(x),
                se,
            }
        })
        .collect()
}

fn loess_points(
    points: &[(f64, f64)],
    options: SmoothOptions,
    x_min: f64,
    x_max: f64,
) -> Vec<SmoothPoint> {
    let n = points.len();
    let span = options.span.clamp(f64::MIN_POSITIVE, 1.0);
    // Neighborhood size: at least 2 points, capped at all of them.
    let q = ((span * n as f64).ceil() as usize).clamp(2, n);

    // Estimate the residual noise from the loess fit at the observed x values so
    // the band reflects the smoother's own residuals (spec §15.x).
    let sigma = if options.se {
        let mut rss = 0.0;
        for (x, y) in points {
            if let Some((yhat, _)) = local_linear(points, *x, q) {
                rss += (y - yhat).powi(2);
            }
        }
        let dof = (n as f64 - 2.0).max(1.0);
        (rss / dof).sqrt()
    } else {
        0.0
    };

    eval_grid(x_min, x_max, options.eval_points)
        .into_iter()
        .filter_map(|x| {
            let (yhat, l2) = local_linear(points, x, q)?;
            Some(SmoothPoint {
                x,
                y: yhat,
                se: sigma * l2.sqrt(),
            })
        })
        .collect()
}

/// Local degree-1 weighted regression at `x0` over the `q` nearest points, using
/// tricube weights. Returns the fitted value and `Σ lᵢ²` (the variance factor of
/// the equivalent kernel), or `None` if the local system is singular.
fn local_linear(points: &[(f64, f64)], x0: f64, q: usize) -> Option<(f64, f64)> {
    let n = points.len();
    if n == 0 {
        return None;
    }
    // The q-th nearest distance sets the tricube bandwidth.
    let mut dists: Vec<f64> = points.iter().map(|(x, _)| (x - x0).abs()).collect();
    dists.sort_by(f64::total_cmp);
    let d_max = dists[q.min(n) - 1].max(f64::MIN_POSITIVE);

    // Weighted normal equations for [intercept, slope].
    let (mut s0, mut s1, mut s2) = (0.0, 0.0, 0.0);
    let (mut t0, mut t1) = (0.0, 0.0);
    let mut weights = vec![0.0; n];
    for (i, (x, y)) in points.iter().enumerate() {
        let u = (x - x0).abs() / d_max;
        let w = if u < 1.0 {
            let t = 1.0 - u * u * u;
            t * t * t
        } else {
            0.0
        };
        weights[i] = w;
        let dx = x - x0;
        s0 += w;
        s1 += w * dx;
        s2 += w * dx * dx;
        t0 += w * y;
        t1 += w * dx * y;
    }
    // Solve the 2x2 system centered at x0, so the prediction is the intercept.
    let det = s0 * s2 - s1 * s1;
    if det.abs() <= f64::EPSILON {
        // Degenerate (e.g. a single distinct neighbor): fall back to the
        // weighted mean.
        if s0 <= f64::EPSILON {
            return None;
        }
        let yhat = t0 / s0;
        let l2: f64 = weights.iter().map(|w| (w / s0).powi(2)).sum();
        return Some((yhat, l2));
    }
    let a = (t0 * s2 - t1 * s1) / det; // intercept = prediction at x0
                                       // l_i = w_i * (s2 - s1 * dx_i) / det; ŷ(x0) = Σ l_i y_i.
    let mut l2 = 0.0;
    for (i, (x, _)) in points.iter().enumerate() {
        let dx = x - x0;
        let li = weights[i] * (s2 - s1 * dx) / det;
        l2 += li * li;
    }
    Some((a, l2))
}

fn bin_layout(min: f64, max: f64, bins: usize, options: BinOptions) -> (f64, f64, usize) {
    if let Some(bin_width) = options.bin_width {
        if bin_width.is_finite() && bin_width > f64::EPSILON {
            let boundary = options.boundary.unwrap_or(bin_width / 2.0);
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

#[cfg(test)]
mod smooth_tests {
    use super::*;

    #[test]
    fn lm_recovers_an_exact_line() {
        // y = 3x + 1; an OLS fit returns the two endpoints exactly.
        let mut points: Vec<(f64, f64)> =
            (0..10).map(|i| (i as f64, 3.0 * i as f64 + 1.0)).collect();
        let out = smooth_points(&mut points, SmoothOptions::default());
        assert_eq!(out.len(), 2);
        assert!((out[0].x - 0.0).abs() < 1e-9 && (out[0].y - 1.0).abs() < 1e-9);
        assert!((out[1].x - 9.0).abs() < 1e-9 && (out[1].y - 28.0).abs() < 1e-9);
    }

    #[test]
    fn lm_se_band_is_widest_at_the_extremes() {
        // The standard error of an OLS fit is minimized at the mean of x and
        // grows toward the ends of the range.
        let mut points: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = i as f64;
                // A little deterministic wiggle so the residual variance is > 0.
                (x, 2.0 * x + if i % 2 == 0 { 1.0 } else { -1.0 })
            })
            .collect();
        let out = smooth_points(
            &mut points,
            SmoothOptions {
                se: true,
                ..SmoothOptions::default()
            },
        );
        assert!(out.len() >= 3);
        let mid = out.len() / 2;
        assert!(out[0].se > out[mid].se);
        assert!(out[out.len() - 1].se > out[mid].se);
        assert!(out.iter().all(|p| p.se >= 0.0));
    }

    #[test]
    fn loess_tracks_a_curve_better_than_a_line() {
        // A parabola: loess should sit much closer to the true curve at its
        // vertex than a single straight OLS fit does.
        let f = |x: f64| (x - 5.0).powi(2);
        let mut data: Vec<(f64, f64)> = (0..=10).map(|i| (i as f64, f(i as f64))).collect();
        let loess = smooth_points(
            &mut data.clone(),
            SmoothOptions {
                method: SmoothMethod::Loess,
                span: 0.5,
                eval_points: 11,
                ..SmoothOptions::default()
            },
        );
        let lm = smooth_points(&mut data, SmoothOptions::default());
        // At x = 5 the true value is 0; loess should be far below the OLS line.
        let loess_at_vertex = loess
            .iter()
            .find(|p| (p.x - 5.0).abs() < 1e-6)
            .map(|p| p.y)
            .unwrap();
        let lm_at_vertex = lm[0].y + (lm[1].y - lm[0].y) * 0.5;
        assert!(loess_at_vertex < lm_at_vertex);
        assert!(loess_at_vertex.abs() < 5.0);
    }

    #[test]
    fn loess_is_deterministic() {
        let f = |x: f64| (x * 0.7).sin();
        let mut a: Vec<(f64, f64)> = (0..30)
            .map(|i| (i as f64 * 0.3, f(i as f64 * 0.3)))
            .collect();
        let mut b = a.clone();
        let opts = SmoothOptions {
            method: SmoothMethod::Loess,
            ..SmoothOptions::default()
        };
        assert_eq!(smooth_points(&mut a, opts), smooth_points(&mut b, opts));
    }

    #[test]
    fn too_few_distinct_x_values_yields_no_points() {
        let mut points = vec![(1.0, 2.0), (1.0, 3.0)];
        assert!(smooth_points(&mut points, SmoothOptions::default()).is_empty());
    }
}

#[cfg(test)]
mod grouped_bin_tests {
    use super::*;

    fn frame(values: &[(f64, &str)]) -> DataFrame {
        DataFrame::new(
            vec![
                col_def("v", DataType::Float),
                col_def("g", DataType::String),
            ],
            vec![
                Column::Float(values.iter().map(|(v, _)| Some(*v)).collect()),
                Column::String(values.iter().map(|(_, g)| Some(g.to_string())).collect()),
            ],
        )
    }

    fn opts() -> BinOptions {
        BinOptions {
            bins: 2,
            bin_width: None,
            boundary: None,
            closed: BinClosed::Left,
            interval: None,
        }
    }

    #[test]
    fn shares_edges_and_stacks_within_each_bin() {
        // Values 0..4 over two groups; both groups must use identical bin edges,
        // and per-bin stacking must be contiguous (lower of one = upper of prev).
        let df = frame(&[
            (0.0, "a"),
            (0.5, "a"),
            (0.5, "b"),
            (3.0, "a"),
            (3.5, "b"),
            (3.5, "b"),
        ]);
        let out = bin_grouped(&df, "v", "g", opts());
        // 2 bins x 2 groups = 4 rows.
        assert_eq!(out.row_count(), 4);
        // All rows reference one of two shared bin_starts.
        let starts: std::collections::BTreeSet<i64> = (0..out.row_count())
            .filter_map(|r| cell_f64(&out, "bin_start", r))
            .map(|v| (v * 1000.0).round() as i64)
            .collect();
        assert_eq!(starts.len(), 2);
        // Within the first bin, stacking is contiguous from zero.
        let mut bin0: Vec<(f64, f64)> = Vec::new();
        for r in 0..out.row_count() {
            if cell_f64(&out, "bin_start", r)
                == Some(starts.iter().next().copied().unwrap() as f64 / 1000.0)
            {
                bin0.push((
                    cell_f64(&out, "stack_lower", r).unwrap(),
                    cell_f64(&out, "stack_upper", r).unwrap(),
                ));
            }
        }
        // First group starts at 0; the next group's lower equals the previous upper.
        assert_eq!(bin0[0].0, 0.0);
        assert_eq!(bin0[0].1, bin0[1].0);
    }

    #[test]
    fn group_order_follows_first_appearance() {
        let df = frame(&[(1.0, "z"), (1.0, "a")]);
        let out = bin_grouped(&df, "v", "g", opts());
        let first_group = match out.value("g", 0) {
            Some(algraf_data::DataValueRef::String(s)) => s.to_string(),
            _ => String::new(),
        };
        assert_eq!(first_group, "z");
    }

    #[test]
    fn dodge_subslots_tile_the_bin_without_overlap() {
        // Two groups: within each bin the dodge slots are adjacent halves
        // [bin_start, mid] and [mid, bin_end], in group order.
        let df = frame(&[(0.2, "a"), (0.2, "b")]);
        let out = bin_grouped(&df, "v", "g", opts());
        // First two rows are bin 0, groups a then b.
        let s0 = cell_f64(&out, "dodge_start", 0).unwrap();
        let e0 = cell_f64(&out, "dodge_end", 0).unwrap();
        let s1 = cell_f64(&out, "dodge_start", 1).unwrap();
        let e1 = cell_f64(&out, "dodge_end", 1).unwrap();
        let bin_start = cell_f64(&out, "bin_start", 0).unwrap();
        let bin_end = cell_f64(&out, "bin_end", 0).unwrap();
        assert!((s0 - bin_start).abs() < 1e-9);
        assert!((e1 - bin_end).abs() < 1e-9);
        // Adjacent, non-overlapping, equal halves.
        assert!((e0 - s1).abs() < 1e-9);
        assert!((e0 - (bin_start + bin_end) / 2.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod blended_bin_tests {
    use super::*;

    fn frame(a: &[Option<f64>], b: &[Option<f64>]) -> DataFrame {
        DataFrame::new(
            vec![col_def("a", DataType::Float), col_def("b", DataType::Float)],
            vec![Column::Float(a.to_vec()), Column::Float(b.to_vec())],
        )
    }

    fn opts() -> BinOptions {
        BinOptions {
            bins: 2,
            bin_width: None,
            boundary: None,
            closed: BinClosed::Left,
            interval: None,
        }
    }

    #[test]
    fn shares_edges_across_all_series_and_preserves_order() {
        let df = frame(
            &[Some(0.0), Some(1.0), Some(2.0)],
            &[Some(8.0), Some(9.0), Some(10.0)],
        );
        let out = bin_blended(&df, &["a", "b"], opts());
        assert_eq!(out.row_count(), 4);
        assert_eq!(cell_f64(&out, "bin_start", 0), Some(0.0));
        assert_eq!(cell_f64(&out, "bin_start", 1), Some(0.0));
        assert_eq!(cell_f64(&out, "bin_start", 2), Some(5.0));
        assert_eq!(cell_category(&out, "series", 0).as_deref(), Some("a"));
        assert_eq!(cell_category(&out, "series", 1).as_deref(), Some("b"));
    }

    #[test]
    fn skips_null_cells_per_series() {
        let df = frame(&[Some(0.0), None], &[None, Some(1.0)]);
        let out = bin_blended(&df, &["a", "b"], BinOptions { bins: 1, ..opts() });
        assert_eq!(out.row_count(), 2);
        assert_eq!(
            out.value("count", 0),
            Some(algraf_data::DataValueRef::Int(1))
        );
        assert_eq!(
            out.value("count", 1),
            Some(algraf_data::DataValueRef::Int(1))
        );
    }

    #[test]
    fn bin_width_without_boundary_centers_integer_values() {
        let df = frame(&[Some(34.0)], &[Some(44.0)]);
        let out = bin_blended(
            &df,
            &["a", "b"],
            BinOptions {
                bin_width: Some(1.0),
                ..opts()
            },
        );
        assert_eq!(cell_f64(&out, "bin_start", 0), Some(33.5));
        assert_eq!(cell_f64(&out, "bin_end", 0), Some(34.5));
        assert_eq!(cell_f64(&out, "bin_center", 0), Some(34.0));
        // Explicit `boundary` continues to anchor bin edges exactly as requested.
        let explicit = bin_blended(
            &df,
            &["a", "b"],
            BinOptions {
                bin_width: Some(1.0),
                boundary: Some(0.0),
                ..opts()
            },
        );
        assert_eq!(cell_f64(&explicit, "bin_start", 0), Some(34.0));
    }
}
