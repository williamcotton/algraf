use algraf_data::TemporalPrecision;
use algraf_semantics::TemporalFormatIr;
use chrono::{DateTime, Datelike, NaiveDate};

use crate::scale::TemporalScale;

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

    if scale.precision == TemporalPrecision::Date {
        if let Some(ticks) = daily_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = monthly_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = yearly_ticks(scale.min, scale.max) {
            return ticks;
        }
    } else {
        if let Some(ticks) = clock_interval_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = daily_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = monthly_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = yearly_ticks(scale.min, scale.max) {
            return ticks;
        }
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

fn monthly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(45..=400).contains(&span_days) {
        return None;
    }

    let (mut year, mut month) = (start.year(), start.month());
    if start.day() > 1 {
        (year, month) = next_month(year, month);
    }

    let mut ticks = Vec::new();
    let mut guard = 0;
    while guard < 60 {
        let micros = month_start_micros(year, month)?;
        if micros > max {
            break;
        }
        if micros >= min {
            ticks.push(micros);
        }
        (year, month) = next_month(year, month);
        guard += 1;
    }

    (2..=8).contains(&ticks.len()).then_some(ticks)
}

fn yearly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if span_days < 365 {
        return None;
    }

    let mut year = start.year();
    if start.ordinal() > 1 {
        year += 1;
    }
    let end_year = end.year();
    let total_years = (end_year - year).max(0);
    let stride = [1, 2, 5, 10]
        .into_iter()
        .find(|stride| total_years / stride < 8)?;

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

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn month_start_micros(year: i32, month: u32) -> Option<i64> {
    Some(
        NaiveDate::from_ymd_opt(year, month, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc()
            .timestamp_micros(),
    )
}
