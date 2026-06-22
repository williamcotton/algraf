use algraf_data::TemporalPrecision;
use algraf_semantics::{TemporalFormatIr, TemporalTickIntervalIr, TemporalTickUnitIr};
use chrono::{DateTime, Datelike, Months, NaiveDate, TimeDelta, Timelike};

use crate::scale::TemporalScale;

const MICROS_PER_MILLISECOND: i64 = 1_000;
const MICROS_PER_SECOND: i64 = 1_000_000;
const MICROS_PER_MINUTE: i64 = 60 * MICROS_PER_SECOND;
const MICROS_PER_HOUR: i64 = 60 * MICROS_PER_MINUTE;
const MICROS_PER_DAY: i64 = 24 * MICROS_PER_HOUR;
const MICROS_PER_WEEK: i64 = 7 * MICROS_PER_DAY;

/// 1970-01-05 was the first ISO Monday on or after the Unix epoch, so the
/// Monday week grid is the epoch grid shifted by four days.
const MONDAY_GRID_OFFSET: i64 = 4 * MICROS_PER_DAY;

/// Explicit `tickInterval` ticks beyond this budget promote the step count
/// (spec §16.11). Wider than the automatic-ladder budget of 8 because an
/// explicit cadence is a deliberate author choice; label overlap is handled
/// by guide-planning label thinning, not by dropping requested ticks.
const MAX_INTERVAL_TICKS: usize = 40;

pub(super) fn format_temporal(
    micros: i64,
    precision: TemporalPrecision,
    format: Option<&TemporalFormatIr>,
) -> String {
    match DateTime::from_timestamp_micros(micros) {
        Some(dt) => match format {
            Some(TemporalFormatIr::IsoDate) => dt.format("%Y-%m-%d").to_string(),
            Some(TemporalFormatIr::IsoMinute) => dt.format("%Y-%m-%d %H:%M").to_string(),
            Some(TemporalFormatIr::IsoSecond) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            Some(TemporalFormatIr::IsoMillis) => dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            Some(TemporalFormatIr::Rfc3339) => dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            Some(TemporalFormatIr::Year) => dt.format("%Y").to_string(),
            Some(TemporalFormatIr::Month) => dt.format("%Y-%m").to_string(),
            Some(TemporalFormatIr::MonthDay) => dt.format("%b %-d").to_string(),
            Some(TemporalFormatIr::TimeMinute) => dt.format("%H:%M").to_string(),
            Some(TemporalFormatIr::TimeSecond) => dt.format("%H:%M:%S").to_string(),
            Some(TemporalFormatIr::Custom(pattern)) => dt.format(pattern).to_string(),
            None => match precision {
                TemporalPrecision::Date => dt.format("%Y-%m-%d").to_string(),
                TemporalPrecision::DateTime => dt.format("%Y-%m-%d %H:%M").to_string(),
            },
        },
        None => String::new(),
    }
}

pub(super) fn temporal_ticks(scale: &TemporalScale) -> Vec<i64> {
    if let Some(ticks) = hinted_temporal_ticks(scale) {
        return ticks;
    }

    if scale.precision != TemporalPrecision::Date {
        if let Some(ticks) = clock_interval_ticks(scale.min, scale.max) {
            return ticks;
        }
    }
    if let Some(ticks) = daily_ticks(scale.min, scale.max) {
        return ticks;
    }
    if let Some(ticks) = weekly_ticks(scale.min, scale.max) {
        return ticks;
    }
    if let Some(ticks) = monthly_ticks(scale.min, scale.max) {
        return ticks;
    }
    if let Some(ticks) = yearly_ticks(scale.min, scale.max) {
        return ticks;
    }

    (0..=5)
        .map(|i| scale.min + (scale.max - scale.min) * i / 5)
        .collect()
}

fn clock_interval_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let span = max.checked_sub(min)?;
    if span <= 0 {
        return None;
    }
    const SECOND: i64 = 1_000_000;
    const MINUTE: i64 = 60 * SECOND;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const INTERVALS: &[i64] = &[
        1_000,
        10_000,
        100_000,
        SECOND,
        5 * SECOND,
        15 * SECOND,
        30 * SECOND,
        MINUTE,
        5 * MINUTE,
        15 * MINUTE,
        30 * MINUTE,
        HOUR,
        6 * HOUR,
        12 * HOUR,
        DAY,
        7 * DAY,
    ];
    for interval in INTERVALS {
        let first = ceil_to_interval(min, *interval)?;
        let count = if first > max {
            0
        } else {
            ((max - first) / interval) + 1
        };
        if (2..=8).contains(&count) {
            return Some((0..count).map(|i| first + i * interval).collect());
        }
    }
    None
}

fn ceil_to_interval(value: i64, interval: i64) -> Option<i64> {
    let rem = value.rem_euclid(interval);
    if rem == 0 {
        Some(value)
    } else {
        value.checked_add(interval - rem)
    }
}

fn hinted_temporal_ticks(scale: &TemporalScale) -> Option<Vec<i64>> {
    let values: Vec<i64> = scale
        .tick_values
        .iter()
        .copied()
        .filter(|value| *value >= scale.min && *value <= scale.max)
        .collect();
    if values.len() < 2 {
        return None;
    }
    if scale.tick_span != Some((scale.min, scale.max)) {
        return None;
    }
    // Ticks generated from an explicit `tickInterval` already honored their
    // own budget through step promotion; index-stride thinning here would
    // break the calendar grid phase (spec §16.11).
    if scale.exact_ticks {
        return Some(values);
    }
    if values.len() <= 8 {
        return Some(values);
    }

    let stride = values.len().div_ceil(8);
    let ticks: Vec<i64> = values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| (index % stride == 0).then_some(value))
        .collect();
    (ticks.len() >= 2).then_some(ticks)
}

fn daily_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(1..=40).contains(&span_days) {
        return None;
    }

    // Pick the smallest stride that produces at most 8 labels, so ticks always
    // land on whole-day boundaries even when the domain isn't a multiple of
    // five days (otherwise the equal-spaced fallback labels a fractional-day
    // position with the truncated date, which reads as misaligned).
    let stride = [1i64, 2, 3, 5, 7, 14]
        .into_iter()
        .find(|s| span_days / s < 8)?;

    let mut ticks = Vec::new();
    let mut offset = 0i64;
    while offset <= span_days {
        let day = start.checked_add_days(chrono::Days::new(offset as u64))?;
        let micros = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_micros();
        if micros >= min && micros <= max {
            ticks.push(micros);
        }
        offset += stride;
    }

    (2..=8).contains(&ticks.len()).then_some(ticks)
}

/// Monday-anchored week boundaries for spans between the daily and monthly
/// ladder rungs (spec §16.4).
fn weekly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(14..=140).contains(&span_days) {
        return None;
    }
    [1i64, 2].into_iter().find_map(|stride| {
        let ticks = epoch_grid_ticks(min, max, stride * MICROS_PER_WEEK, MONDAY_GRID_OFFSET);
        (2..=8).contains(&ticks.len()).then_some(ticks)
    })
}

/// Month-start boundaries with 1-, 2-, 3-, and 6-month strides on the epoch
/// month grid, so multi-month strides keep the same calendar phase every
/// year (a 3-month stride reads January/April/July/October, spec §16.4).
fn monthly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if span_days < 28 {
        return None;
    }
    [1i64, 2, 3, 6].into_iter().find_map(|stride| {
        let ticks = month_grid_ticks(min, max, stride);
        (2..=8).contains(&ticks.len()).then_some(ticks)
    })
}

fn yearly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if span_days < 365 {
        return None;
    }

    let mut first_year = start.year();
    if start.ordinal() > 1 {
        first_year += 1;
    }
    let end_year = end.year();
    let total_years = (end_year - first_year).max(0);
    let stride = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000]
        .into_iter()
        .find(|stride| total_years / stride < 8)?;

    let mut year = first_year;
    let mut ticks = Vec::new();
    while year <= end_year {
        let micros = month_start_micros(year, 1)?;
        if micros >= min && micros <= max {
            ticks.push(micros);
        }
        year += stride;
    }
    (2..=8).contains(&ticks.len()).then_some(ticks)
}

/// Generate grid-anchored calendar/clock ticks for an explicit
/// `Scale(tickInterval: ...)` (spec §16.11). When the requested cadence
/// exceeds the interval tick budget, the step count is promoted by the
/// smallest integer multiple that fits, preserving the unit grid phase.
pub(crate) fn interval_ticks(min: i64, max: i64, interval: TemporalTickIntervalIr) -> Vec<i64> {
    interval_ticks_with_step(min, max, interval).0
}

/// The effective step count `interval_ticks` settled on after promotion.
/// Equal to `interval.count` when no promotion was needed.
pub(crate) fn interval_effective_count(
    min: i64,
    max: i64,
    interval: TemporalTickIntervalIr,
) -> i64 {
    interval_ticks_with_step(min, max, interval).1
}

/// Advance an arbitrary UTC-equivalent instant by the authored temporal
/// interval. Temporal bars use this for bucket endpoints; unlike tick
/// generation, the advance is relative to the row's anchor value rather than a
/// global grid boundary.
pub(crate) fn advance_by_interval(micros: i64, interval: TemporalTickIntervalIr) -> Option<i64> {
    let count = i64::from(interval.count.max(1));
    match interval.unit {
        TemporalTickUnitIr::Millisecond => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_MILLISECOND)?)
        }
        TemporalTickUnitIr::Second => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_SECOND)?)
        }
        TemporalTickUnitIr::Minute => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_MINUTE)?)
        }
        TemporalTickUnitIr::Hour => add_fixed_micros(micros, count.checked_mul(MICROS_PER_HOUR)?),
        TemporalTickUnitIr::Day => add_fixed_micros(micros, count.checked_mul(MICROS_PER_DAY)?),
        TemporalTickUnitIr::Week => add_fixed_micros(micros, count.checked_mul(MICROS_PER_WEEK)?),
        TemporalTickUnitIr::Month => add_calendar_months(micros, count),
        TemporalTickUnitIr::Quarter => add_calendar_months(micros, count.checked_mul(3)?),
        TemporalTickUnitIr::Year => add_calendar_months(micros, count.checked_mul(12)?),
    }
}

/// Move an arbitrary UTC-equivalent instant backward by the authored temporal
/// interval, using calendar arithmetic for month-like units.
pub(crate) fn retreat_by_interval(micros: i64, interval: TemporalTickIntervalIr) -> Option<i64> {
    let count = i64::from(interval.count.max(1));
    match interval.unit {
        TemporalTickUnitIr::Millisecond => add_fixed_micros(
            micros,
            count.checked_mul(MICROS_PER_MILLISECOND)?.checked_neg()?,
        ),
        TemporalTickUnitIr::Second => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_SECOND)?.checked_neg()?)
        }
        TemporalTickUnitIr::Minute => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_MINUTE)?.checked_neg()?)
        }
        TemporalTickUnitIr::Hour => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_HOUR)?.checked_neg()?)
        }
        TemporalTickUnitIr::Day => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_DAY)?.checked_neg()?)
        }
        TemporalTickUnitIr::Week => {
            add_fixed_micros(micros, count.checked_mul(MICROS_PER_WEEK)?.checked_neg()?)
        }
        TemporalTickUnitIr::Month => subtract_calendar_months(micros, count),
        TemporalTickUnitIr::Quarter => subtract_calendar_months(micros, count.checked_mul(3)?),
        TemporalTickUnitIr::Year => subtract_calendar_months(micros, count.checked_mul(12)?),
    }
}

/// Resolve a temporal bar bucket centered on the row anchor. Fixed intervals
/// become anchor +/- half the interval; month/quarter/year intervals use the
/// midpoint between neighboring calendar anchors.
pub(crate) fn centered_bucket_bounds(
    micros: i64,
    interval: TemporalTickIntervalIr,
) -> Option<(i64, i64)> {
    let previous = retreat_by_interval(micros, interval)?;
    let next = advance_by_interval(micros, interval)?;
    Some((
        midpoint_micros(previous, micros),
        midpoint_micros(micros, next),
    ))
}

fn add_fixed_micros(micros: i64, delta_micros: i64) -> Option<i64> {
    Some(
        DateTime::from_timestamp_micros(micros)?
            .checked_add_signed(TimeDelta::microseconds(delta_micros))?
            .timestamp_micros(),
    )
}

fn add_calendar_months(micros: i64, months: i64) -> Option<i64> {
    let months = u32::try_from(months).ok()?;
    Some(
        DateTime::from_timestamp_micros(micros)?
            .checked_add_months(Months::new(months))?
            .timestamp_micros(),
    )
}

fn subtract_calendar_months(micros: i64, months: i64) -> Option<i64> {
    let months = u32::try_from(months).ok()?;
    Some(
        DateTime::from_timestamp_micros(micros)?
            .checked_sub_months(Months::new(months))?
            .timestamp_micros(),
    )
}

fn midpoint_micros(a: i64, b: i64) -> i64 {
    let sum = i128::from(a) + i128::from(b);
    (sum / 2) as i64
}

fn interval_ticks_with_step(
    min: i64,
    max: i64,
    interval: TemporalTickIntervalIr,
) -> (Vec<i64>, i64) {
    let base = i64::from(interval.count.max(1));
    let span = max.saturating_sub(min).max(1);
    // Jump close to a fitting multiplier so sub-second intervals over long
    // domains converge without millions of generation passes.
    let estimated_ticks = span
        / approx_step_micros(interval.unit)
            .saturating_mul(base)
            .max(1);
    let mut multiplier = (estimated_ticks / MAX_INTERVAL_TICKS as i64).max(1);
    loop {
        let count = base.saturating_mul(multiplier);
        let ticks = interval_grid_ticks(min, max, count, interval.unit);
        if ticks.len() <= MAX_INTERVAL_TICKS {
            return (ticks, count);
        }
        multiplier += 1;
    }
}

fn approx_step_micros(unit: TemporalTickUnitIr) -> i64 {
    match unit {
        TemporalTickUnitIr::Millisecond => MICROS_PER_MILLISECOND,
        TemporalTickUnitIr::Second => MICROS_PER_SECOND,
        TemporalTickUnitIr::Minute => MICROS_PER_MINUTE,
        TemporalTickUnitIr::Hour => MICROS_PER_HOUR,
        TemporalTickUnitIr::Day => MICROS_PER_DAY,
        TemporalTickUnitIr::Week => MICROS_PER_WEEK,
        TemporalTickUnitIr::Month => 30 * MICROS_PER_DAY,
        TemporalTickUnitIr::Quarter => 91 * MICROS_PER_DAY,
        TemporalTickUnitIr::Year => 365 * MICROS_PER_DAY,
    }
}

fn interval_grid_ticks(min: i64, max: i64, count: i64, unit: TemporalTickUnitIr) -> Vec<i64> {
    match unit {
        TemporalTickUnitIr::Millisecond => {
            day_anchored_clock_ticks(min, max, count.saturating_mul(MICROS_PER_MILLISECOND))
        }
        TemporalTickUnitIr::Second => {
            day_anchored_clock_ticks(min, max, count.saturating_mul(MICROS_PER_SECOND))
        }
        TemporalTickUnitIr::Minute => {
            day_anchored_clock_ticks(min, max, count.saturating_mul(MICROS_PER_MINUTE))
        }
        TemporalTickUnitIr::Hour => {
            day_anchored_clock_ticks(min, max, count.saturating_mul(MICROS_PER_HOUR))
        }
        TemporalTickUnitIr::Day => {
            epoch_grid_ticks(min, max, count.saturating_mul(MICROS_PER_DAY), 0)
        }
        TemporalTickUnitIr::Week => epoch_grid_ticks(
            min,
            max,
            count.saturating_mul(MICROS_PER_WEEK),
            MONDAY_GRID_OFFSET,
        ),
        TemporalTickUnitIr::Month => month_grid_ticks(min, max, count),
        TemporalTickUnitIr::Quarter => month_grid_ticks(min, max, count.saturating_mul(3)),
        TemporalTickUnitIr::Year => year_grid_ticks(min, max, count),
    }
}

/// Clock-unit steps restart at every UTC-equivalent midnight, so `"6 hours"`
/// always lands on 00:00, 06:00, 12:00, and 18:00 regardless of the domain.
fn day_anchored_clock_ticks(min: i64, max: i64, step: i64) -> Vec<i64> {
    let mut ticks = Vec::new();
    if step <= 0 || min > max {
        return ticks;
    }
    let mut day = min
        .div_euclid(MICROS_PER_DAY)
        .saturating_mul(MICROS_PER_DAY);
    'days: while day <= max {
        let day_end = day.saturating_add(MICROS_PER_DAY);
        let mut tick = day;
        while tick < day_end {
            if tick >= min && tick <= max {
                ticks.push(tick);
                if ticks.len() > MAX_INTERVAL_TICKS {
                    break 'days;
                }
            }
            tick = tick.saturating_add(step);
        }
        day = day_end;
    }
    ticks
}

/// Fixed-duration steps on the Unix epoch grid, optionally phase-shifted
/// (the ISO Monday week grid is the epoch grid shifted by four days).
fn epoch_grid_ticks(min: i64, max: i64, step: i64, phase: i64) -> Vec<i64> {
    let mut ticks = Vec::new();
    if step <= 0 || min > max {
        return ticks;
    }
    let rem = (min.saturating_sub(phase)).rem_euclid(step);
    let mut tick = if rem == 0 {
        min
    } else {
        min.saturating_add(step - rem)
    };
    while tick <= max {
        ticks.push(tick);
        if ticks.len() > MAX_INTERVAL_TICKS {
            break;
        }
        tick = tick.saturating_add(step);
    }
    ticks
}

/// Month steps on the epoch month grid (January 1970). Steps that divide 12 —
/// including quarters — land on the same months every year, e.g. `"3 months"`
/// is January/April/July/October.
fn month_grid_ticks(min: i64, max: i64, count: i64) -> Vec<i64> {
    let mut ticks = Vec::new();
    if count <= 0 || min > max {
        return ticks;
    }
    let Some(start) = DateTime::from_timestamp_micros(min) else {
        return ticks;
    };
    let start = start.date_naive();
    let mut index = i64::from(start.year() - 1970) * 12 + i64::from(start.month0());
    index -= index.rem_euclid(count);
    loop {
        let year = 1970 + index.div_euclid(12);
        let month = index.rem_euclid(12) + 1;
        let Ok(year) = i32::try_from(year) else {
            break;
        };
        let Some(micros) = month_start_micros(year, month as u32) else {
            break;
        };
        if micros > max {
            break;
        }
        if micros >= min {
            ticks.push(micros);
            if ticks.len() > MAX_INTERVAL_TICKS {
                break;
            }
        }
        index += count;
    }
    ticks
}

/// Year steps land on years divisible by the step count, so `"5 years"`
/// ticks read 1995, 2000, 2005 rather than arbitrary domain-relative years.
fn year_grid_ticks(min: i64, max: i64, count: i64) -> Vec<i64> {
    let mut ticks = Vec::new();
    if count <= 0 || min > max {
        return ticks;
    }
    let Some(start) = DateTime::from_timestamp_micros(min) else {
        return ticks;
    };
    let mut year = i64::from(start.date_naive().year());
    year -= year.rem_euclid(count);
    while let Ok(y) = i32::try_from(year) {
        let Some(micros) = month_start_micros(y, 1) else {
            break;
        };
        if micros > max {
            break;
        }
        if micros >= min {
            ticks.push(micros);
            if ticks.len() > MAX_INTERVAL_TICKS {
                break;
            }
        }
        year += count;
    }
    ticks
}

/// Pick a deterministic default label pattern from the granularity of the
/// generated ticks (spec §16.4): year-start ticks read `2024`, month-start
/// ticks read `2024-04`, and other day-aligned ticks read `2024-04-15`.
/// Sub-day ticks defer to the precision-based default.
pub(super) fn default_tick_pattern(ticks: &[i64]) -> Option<&'static str> {
    if ticks.len() < 2 {
        return None;
    }
    let mut all_year_starts = true;
    let mut all_month_starts = true;
    for micros in ticks {
        let datetime = DateTime::from_timestamp_micros(*micros)?;
        let midnight =
            datetime.time().num_seconds_from_midnight() == 0 && datetime.time().nanosecond() == 0;
        if !midnight {
            return None;
        }
        let date = datetime.date_naive();
        all_month_starts &= date.day() == 1;
        all_year_starts &= date.day() == 1 && date.month() == 1;
    }
    if all_year_starts {
        Some("%Y")
    } else if all_month_starts {
        Some("%Y-%m")
    } else {
        Some("%Y-%m-%d")
    }
}

/// Format one tick with an adaptive default pattern from
/// [`default_tick_pattern`].
pub(super) fn format_with_pattern(micros: i64, pattern: &str) -> String {
    DateTime::from_timestamp_micros(micros)
        .map(|datetime| datetime.format(pattern).to_string())
        .unwrap_or_default()
}

fn month_start_micros(year: i32, month: u32) -> Option<i64> {
    Some(
        NaiveDate::from_ymd_opt(year, month, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc()
            .timestamp_micros(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn micros(date: &str) -> i64 {
        let datetime = if date.len() == 10 {
            format!("{date}T00:00:00Z")
        } else {
            format!("{date}Z")
        };
        DateTime::parse_from_rfc3339(&datetime)
            .expect("test timestamp parses")
            .timestamp_micros()
    }

    fn interval(count: u32, unit: TemporalTickUnitIr) -> TemporalTickIntervalIr {
        TemporalTickIntervalIr { count, unit }
    }

    fn dates(ticks: &[i64]) -> Vec<String> {
        ticks
            .iter()
            .map(|tick| format_with_pattern(*tick, "%Y-%m-%d"))
            .collect()
    }

    /// Quarter and 3-month cadences share the epoch month grid, so both land
    /// on January/April/July/October regardless of where the domain starts.
    #[test]
    fn quarterly_interval_ticks_land_on_calendar_quarters() {
        let min = micros("2022-05-21");
        let max = micros("2025-09-15");
        let months = interval_ticks(min, max, interval(3, TemporalTickUnitIr::Month));
        let quarters = interval_ticks(min, max, interval(1, TemporalTickUnitIr::Quarter));
        assert_eq!(months, quarters);
        assert_eq!(dates(&months)[0], "2022-07-01");
        assert_eq!(dates(&months).last().unwrap(), "2025-07-01");
        assert!(dates(&months)
            .iter()
            .all(|date| ["-01-01", "-04-01", "-07-01", "-10-01"]
                .iter()
                .any(|suffix| date.ends_with(suffix))));
    }

    /// Week ticks anchor to the ISO Monday grid, not the domain start.
    #[test]
    fn weekly_interval_ticks_anchor_to_monday() {
        let ticks = interval_ticks(
            micros("2025-02-19"), // Wednesday
            micros("2025-03-19"),
            interval(1, TemporalTickUnitIr::Week),
        );
        assert_eq!(
            dates(&ticks),
            ["2025-02-24", "2025-03-03", "2025-03-10", "2025-03-17"]
        );
    }

    /// Year steps land on years divisible by the count.
    #[test]
    fn multi_year_interval_ticks_use_divisible_years() {
        let ticks = interval_ticks(
            micros("1997-06-01"),
            micros("2013-02-01"),
            interval(5, TemporalTickUnitIr::Year),
        );
        assert_eq!(dates(&ticks), ["2000-01-01", "2005-01-01", "2010-01-01"]);
    }

    /// Clock steps restart at midnight, so 6-hour ticks always read
    /// 00/06/12/18 even when the domain starts mid-afternoon.
    #[test]
    fn clock_interval_ticks_anchor_to_midnight() {
        let ticks = interval_ticks(
            micros("2025-03-01T15:30:00"),
            micros("2025-03-02T13:00:00"),
            interval(6, TemporalTickUnitIr::Hour),
        );
        let labels: Vec<String> = ticks
            .iter()
            .map(|tick| format_with_pattern(*tick, "%d %H:%M"))
            .collect();
        assert_eq!(labels, ["01 18:00", "02 00:00", "02 06:00", "02 12:00"]);
    }

    /// A cadence far over the budget promotes the step by an integer
    /// multiple on the same grid instead of index-thinning.
    #[test]
    fn interval_ticks_promote_step_over_budget() {
        let min = micros("2020-01-01");
        let max = micros("2025-01-01");
        let ticks = interval_ticks(min, max, interval(1, TemporalTickUnitIr::Day));
        assert!(ticks.len() <= MAX_INTERVAL_TICKS);
        assert!(ticks.len() >= 2);
        let effective = interval_effective_count(min, max, interval(1, TemporalTickUnitIr::Day));
        assert!(effective > 1, "five years of daily ticks must promote");
        // Promotion preserves the epoch day grid: consecutive ticks stay
        // exactly `effective` days apart.
        for pair in ticks.windows(2) {
            assert_eq!(pair[1] - pair[0], effective * MICROS_PER_DAY);
        }
    }

    fn date_scale(min: &str, max: &str) -> TemporalScale {
        TemporalScale::new(
            micros(min),
            micros(max),
            (0.0, 100.0),
            TemporalPrecision::Date,
        )
    }

    /// An 18-month domain used to fall through the monthly (too many) and
    /// yearly (too few) rungs to equal-spaced numeric interpolation; the
    /// extended ladder now produces month-grid ticks.
    #[test]
    fn ladder_covers_eighteen_month_spans_with_calendar_ticks() {
        let scale = date_scale("2024-01-15", "2025-07-15");
        let ticks = temporal_ticks(&scale);
        assert!((2..=8).contains(&ticks.len()));
        for date in dates(&ticks) {
            assert!(date.ends_with("-01"), "{date} is not a month start");
        }
    }

    /// Multi-year spans get multi-month strides on the epoch month grid
    /// instead of jumping straight to sparse yearly ticks.
    #[test]
    fn ladder_uses_multi_month_strides_for_three_year_spans() {
        let scale = date_scale("2022-05-21", "2025-09-15");
        let ticks = temporal_ticks(&scale);
        assert!((4..=8).contains(&ticks.len()), "got {}", ticks.len());
        assert!(dates(&ticks)
            .iter()
            .all(|date| date.ends_with("-01-01") || date.ends_with("-07-01")));
    }

    /// Two-month spans get Monday-anchored weekly ticks between the daily
    /// and monthly rungs.
    #[test]
    fn ladder_uses_monday_weeks_for_two_month_spans() {
        let scale = date_scale("2025-01-04", "2025-03-04");
        let ticks = temporal_ticks(&scale);
        assert!((2..=8).contains(&ticks.len()));
        for tick in &ticks {
            let weekday = DateTime::from_timestamp_micros(*tick)
                .expect("tick is a valid instant")
                .date_naive()
                .weekday();
            assert_eq!(weekday, chrono::Weekday::Mon);
        }
    }

    /// A century-long span stays on calendar year boundaries rather than
    /// falling back to numeric interpolation.
    #[test]
    fn ladder_covers_century_spans_with_year_ticks() {
        let scale = date_scale("1925-03-01", "2025-03-01");
        let ticks = temporal_ticks(&scale);
        assert!((2..=8).contains(&ticks.len()));
        for date in dates(&ticks) {
            assert!(date.ends_with("-01-01"), "{date} is not a year start");
        }
    }

    /// Default labels adapt to tick granularity: year starts read `%Y`,
    /// month starts read `%Y-%m`, other day-aligned ticks read `%Y-%m-%d`.
    #[test]
    fn default_tick_pattern_adapts_to_granularity() {
        let years = [micros("2023-01-01"), micros("2024-01-01")];
        let months = [micros("2024-01-01"), micros("2024-04-01")];
        let days = [micros("2024-01-01"), micros("2024-01-15")];
        let clock = [micros("2024-01-01T06:00:00"), micros("2024-01-01T12:00:00")];
        assert_eq!(default_tick_pattern(&years), Some("%Y"));
        assert_eq!(default_tick_pattern(&months), Some("%Y-%m"));
        assert_eq!(default_tick_pattern(&days), Some("%Y-%m-%d"));
        assert_eq!(default_tick_pattern(&clock), None);
    }

    /// Explicit interval ticks bypass index-stride thinning.
    #[test]
    fn exact_ticks_skip_index_stride_thinning() {
        let mut scale = date_scale("2022-05-21", "2025-09-15");
        let ticks = interval_ticks(scale.min, scale.max, interval(3, TemporalTickUnitIr::Month));
        let expected = ticks.len();
        assert!(
            expected > 8,
            "quarterly over 3.4 years exceeds the auto budget"
        );
        scale.tick_values = ticks;
        scale.tick_span = Some((scale.min, scale.max));
        scale.exact_ticks = true;
        assert_eq!(temporal_ticks(&scale).len(), expected);
    }
}
