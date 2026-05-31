//! Scales and nice ticks (spec §16).

use algraf_data::{DataValueRef, Table, TemporalPrecision};

/// A linear continuous scale mapping a numeric domain to a pixel range
/// (spec §16.3).
#[derive(Debug, Clone)]
pub struct ContinuousScale {
    pub min: f64,
    pub max: f64,
    pub range: (f64, f64),
    pub transform: ContinuousTransform,
    /// Constrain ticks to whole integers (`Scale(integer: true)`, spec §16.10).
    pub integer: bool,
    /// Exact user-specified tick values, if any.
    pub tick_values: Vec<f64>,
    /// Labels paired with `tick_values`.
    pub tick_labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuousTransform {
    Linear,
    Log10,
    /// Square-root transform for non-negative continuous position axes
    /// (spec §16.2). Ticks are nice data values positioned by `sqrt`.
    Sqrt,
}

impl ContinuousScale {
    pub fn new(min: f64, max: f64, range: (f64, f64)) -> Self {
        Self::with_transform(min, max, range, ContinuousTransform::Linear)
    }

    pub fn log10(min: f64, max: f64, range: (f64, f64)) -> Self {
        Self::with_transform(min, max, range, ContinuousTransform::Log10)
    }

    pub fn sqrt(min: f64, max: f64, range: (f64, f64)) -> Self {
        Self::with_transform(min, max, range, ContinuousTransform::Sqrt)
    }

    fn with_transform(
        min: f64,
        max: f64,
        range: (f64, f64),
        transform: ContinuousTransform,
    ) -> Self {
        // Handle zero-width domains by expanding symmetrically (spec §16.3).
        let (min, max) = if (max - min).abs() < f64::EPSILON {
            if min == 0.0 {
                (-0.5, 0.5)
            } else {
                (min - min.abs() * 0.5, max + max.abs() * 0.5)
            }
        } else {
            (min, max)
        };
        ContinuousScale {
            min,
            max,
            range,
            transform,
            integer: false,
            tick_values: Vec::new(),
            tick_labels: Vec::new(),
        }
    }

    pub fn map(&self, value: f64) -> f64 {
        let t = match self.transform {
            ContinuousTransform::Linear => (value - self.min) / (self.max - self.min),
            ContinuousTransform::Log10 => {
                let min = self.min.log10();
                let max = self.max.log10();
                (value.log10() - min) / (max - min)
            }
            ContinuousTransform::Sqrt => {
                let min = self.min.max(0.0).sqrt();
                let max = self.max.max(0.0).sqrt();
                (value.max(0.0).sqrt() - min) / (max - min)
            }
        };
        self.range.0 + t * (self.range.1 - self.range.0)
    }

    pub fn ticks(&self, target: usize) -> Vec<f64> {
        if !self.tick_values.is_empty() {
            return self
                .tick_values
                .iter()
                .copied()
                .filter(|t| *t >= self.min - f64::EPSILON && *t <= self.max + f64::EPSILON)
                .collect();
        }
        match self.transform {
            ContinuousTransform::Linear if self.integer => {
                integer_ticks(self.min, self.max, target)
            }
            ContinuousTransform::Linear => nice_ticks(self.min, self.max, target),
            ContinuousTransform::Log10 => log_ticks(self.min, self.max),
            // Sqrt ticks are nice data values; the `map` above positions them on
            // the square-root axis (spec §16.2).
            ContinuousTransform::Sqrt => nice_ticks(self.min, self.max, target),
        }
    }
}

/// A temporal scale mapping instants (microseconds) to a pixel range
/// (spec §16.4). Mapping is purely linear over UTC-equivalent instants.
#[derive(Debug, Clone)]
pub struct TemporalScale {
    pub min: i64,
    pub max: i64,
    pub range: (f64, f64),
    pub precision: TemporalPrecision,
    pub tick_values: Vec<i64>,
    pub tick_labels: Vec<String>,
    pub tick_span: Option<(i64, i64)>,
}

impl TemporalScale {
    pub fn new(min: i64, max: i64, range: (f64, f64), precision: TemporalPrecision) -> Self {
        let (min, max) = if min == max {
            (min - 1, max + 1)
        } else {
            (min, max)
        };
        TemporalScale {
            min,
            max,
            range,
            precision,
            tick_values: Vec::new(),
            tick_labels: Vec::new(),
            tick_span: None,
        }
    }

    pub fn map(&self, micros: i64) -> f64 {
        let t = (micros - self.min) as f64 / (self.max - self.min) as f64;
        self.range.0 + t * (self.range.1 - self.range.0)
    }
}

/// A categorical band scale (spec §16.5).
#[derive(Debug, Clone)]
pub struct BandScale {
    pub categories: Vec<String>,
    pub range: (f64, f64),
    pub pad_inner: f64,
    pub pad_outer: f64,
}

impl BandScale {
    pub fn new(categories: Vec<String>, range: (f64, f64)) -> Self {
        BandScale {
            categories,
            range,
            pad_inner: 0.2,
            pad_outer: 0.1,
        }
    }

    fn step(&self) -> f64 {
        let n = self.categories.len().max(1) as f64;
        (self.range.1 - self.range.0) / (n - self.pad_inner + 2.0 * self.pad_outer)
    }

    pub fn bandwidth(&self) -> f64 {
        (self.step() * (1.0 - self.pad_inner)).abs()
    }

    /// The `(start, width)` of the band for a category, if present.
    pub fn band(&self, category: &str) -> Option<(f64, f64)> {
        let index = self.categories.iter().position(|c| c == category)?;
        let step = self.step();
        let start = self.range.0 + self.pad_outer * step + index as f64 * step;
        let width = self.bandwidth();
        // Normalize so `start` is the lower pixel coordinate.
        if step >= 0.0 {
            Some((start, width))
        } else {
            Some((start - width, width))
        }
    }

    pub fn center(&self, category: &str) -> Option<f64> {
        self.band(category)
            .map(|(start, width)| start + width / 2.0)
    }
}

/// A nested band scale: inner bands within each outer band (spec §16.6).
#[derive(Debug, Clone)]
pub struct NestedBandScale {
    pub outer: BandScale,
    pub inner_categories: Vec<String>,
    pub pad_inner: f64,
}

impl NestedBandScale {
    pub fn new(outer: BandScale, inner_categories: Vec<String>) -> Self {
        NestedBandScale {
            outer,
            inner_categories,
            pad_inner: 0.1,
        }
    }

    /// The `(start, width)` for an `(outer, inner)` category pair.
    pub fn band(&self, outer_cat: &str, inner_cat: &str) -> Option<(f64, f64)> {
        let (outer_start, outer_width) = self.outer.band(outer_cat)?;
        let inner = BandScale {
            categories: self.inner_categories.clone(),
            range: (0.0, outer_width),
            pad_inner: self.pad_inner,
            pad_outer: 0.0,
        };
        let (inner_start, inner_width) = inner.band(inner_cat)?;
        Some((outer_start + inner_start, inner_width))
    }
}

// --- Domain collection -----------------------------------------------------

/// Read a cell as `f64` (Int or Float), or `None` for missing/non-numeric.
pub fn cell_f64(table: &dyn Table, column: &str, row: usize) -> Option<f64> {
    match table.value(column, row)? {
        DataValueRef::Int(i) => Some(i as f64),
        DataValueRef::Float(f) if f.is_finite() => Some(f),
        _ => None,
    }
}

/// Read a cell as a temporal instant in microseconds.
pub fn cell_micros(table: &dyn Table, column: &str, row: usize) -> Option<i64> {
    match table.value(column, row)? {
        DataValueRef::Temporal(t) => Some(t.instant.and_utc().timestamp_micros()),
        _ => None,
    }
}

/// Read a cell as a category key string, or `None` for missing.
pub fn cell_category(table: &dyn Table, column: &str, row: usize) -> Option<String> {
    match table.value(column, row)? {
        DataValueRef::Null => None,
        DataValueRef::Bool(b) => Some(b.to_string()),
        DataValueRef::Int(i) => Some(i.to_string()),
        DataValueRef::Float(f) => Some(crate::svg::num(f)),
        DataValueRef::Temporal(t) => Some(t.instant.and_utc().to_rfc3339()),
        DataValueRef::String(s) => Some(s.to_string()),
        // Geometry is not a categorical domain (spec §10.11).
        DataValueRef::Geometry(_) => None,
    }
}

/// The numeric `(min, max)` over a column's finite values.
pub fn numeric_domain(table: &dyn Table, column: &str) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for row in 0..table.row_count() {
        if let Some(v) = cell_f64(table, column, row) {
            min = min.min(v);
            max = max.max(v);
        }
    }
    (min <= max).then_some((min, max))
}

/// The temporal `(min, max, precision)` over a column.
pub fn temporal_domain(table: &dyn Table, column: &str) -> Option<(i64, i64, TemporalPrecision)> {
    let mut min = i64::MAX;
    let mut max = i64::MIN;
    let mut precision = TemporalPrecision::Date;
    let mut seen = false;
    for row in 0..table.row_count() {
        if let Some(DataValueRef::Temporal(t)) = table.value(column, row) {
            seen = true;
            let micros = t.instant.and_utc().timestamp_micros();
            min = min.min(micros);
            max = max.max(micros);
            if t.precision == TemporalPrecision::DateTime {
                precision = TemporalPrecision::DateTime;
            }
        }
    }
    seen.then_some((min, max, precision))
}

/// Unique categories in first-appearance order (deterministic).
pub fn categorical_domain(table: &dyn Table, column: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for row in 0..table.row_count() {
        if let Some(cat) = cell_category(table, column, row) {
            if !seen.contains(&cat) {
                seen.push(cat);
            }
        }
    }
    seen
}

// --- Nice ticks ------------------------------------------------------------

/// Generate up to ~`target` nice ticks across `[min, max]` (spec §16.10).
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || target == 0 {
        return vec![];
    }
    if (max - min).abs() < f64::EPSILON {
        return vec![min];
    }
    let span = max - min;
    if min.fract().abs() < f64::EPSILON
        && max.fract().abs() < f64::EPSILON
        && span.abs() <= target as f64 + 2.0
    {
        let mut ticks = Vec::new();
        let mut value = min.ceil();
        while value <= max + f64::EPSILON && ticks.len() < target + 3 {
            ticks.push(value);
            value += 1.0;
        }
        return ticks;
    }

    let step = nice_step(span, target);

    let start = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = start;
    // Guard against runaway loops.
    let mut guard = 0;
    while value <= max + step * 1e-9 && guard < 1000 {
        // Snap near-zero values to exactly zero.
        ticks.push(if value.abs() < step * 1e-9 {
            0.0
        } else {
            value
        });
        value += step;
        guard += 1;
    }
    ticks
}

/// Generate ticks constrained to whole integers across `[min, max]`
/// (`Scale(integer: true)`, spec §16.10). The step is the nice step rounded up
/// to at least 1, so small ranges land on consecutive integers and large ones
/// keep a human-friendly integer stride (1, 2, 5, 10, …).
pub fn integer_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || target == 0 {
        return vec![];
    }
    let span = max - min;
    let step = if span.abs() < f64::EPSILON {
        1.0
    } else {
        nice_step(span, target).max(1.0).round()
    };
    let start = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = start;
    let mut guard = 0;
    while value <= max + step * 1e-9 && guard < 1000 {
        ticks.push(if value.abs() < step * 1e-9 {
            0.0
        } else {
            value
        });
        value += step;
        guard += 1;
    }
    // A range narrower than a single integer step (e.g. all observations equal)
    // still deserves one labelled tick.
    if ticks.is_empty() {
        ticks.push(min.round());
    }
    ticks
}

/// Pick a deterministic human-friendly step for a numeric span.
pub fn nice_step(span: f64, target: usize) -> f64 {
    let raw_step = span.abs() / target.max(1) as f64;
    if raw_step <= f64::EPSILON || !raw_step.is_finite() {
        return 1.0;
    }
    let magnitude = 10f64.powf(raw_step.abs().log10().floor());
    let normalized = raw_step / magnitude;
    let nice = if normalized < 1.5 {
        1.0
    } else if normalized < 3.0 {
        2.0
    } else if normalized < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

fn log_ticks(min: f64, max: f64) -> Vec<f64> {
    if min <= 0.0 || max <= 0.0 || !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }

    let start = min.log10().floor() as i32;
    let end = max.log10().ceil() as i32;
    let mut ticks = Vec::new();
    for power in start..=end {
        let value = 10f64.powi(power);
        if value >= min - f64::EPSILON && value <= max + f64::EPSILON {
            ticks.push(value);
        }
    }

    if ticks.is_empty() {
        vec![min, max]
    } else {
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqrt_scale_positions_by_square_root() {
        let scale = ContinuousScale::sqrt(0.0, 100.0, (0.0, 100.0));
        // 25 → sqrt(25)/sqrt(100) = 0.5 of the range.
        assert!((scale.map(25.0) - 50.0).abs() < 1e-9);
        assert!((scale.map(0.0) - 0.0).abs() < 1e-9);
        assert!((scale.map(100.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn sqrt_scale_uses_nice_data_value_ticks() {
        let scale = ContinuousScale::sqrt(0.0, 100.0, (0.0, 1.0));
        let ticks = scale.ticks(5);
        assert!(ticks.contains(&100.0));
        // Ticks are evenly spaced data values, not log decades.
        assert!(ticks.windows(2).all(|w| (w[1] - w[0] - 20.0).abs() < 1e-9));
    }

    #[test]
    fn sqrt_scale_clamps_negative_input() {
        let scale = ContinuousScale::sqrt(0.0, 100.0, (0.0, 100.0));
        assert!(scale.map(-4.0).is_finite());
    }
}
