//! The `Graticule` spatial guide mark (spec §14.24).
//!
//! A graticule draws the projected longitude/latitude grid. Each meridian and
//! parallel is sampled finely in geographic space and projected through the
//! active [`SpatialScale`](crate::projection::SpatialScale), so curved
//! projections (Mercator, Albers, …) render as smooth curves. Lines are emitted
//! in a deterministic order — meridians west→east, then parallels south→north —
//! so output is stable (spec §18.12).

use std::fmt::Write;

use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::number_setting;
use crate::projection::SpatialScale;
use crate::sink::MarkSink;
use crate::svg::num;

use super::common::opacity_when_translucent;
use super::GeometryRenderContext;

/// Latitude clamp keeping samples away from projection singularities at the poles.
const LAT_LIMIT: f64 = 89.5;

/// Render the graticule for the active spatial space.
pub(super) fn render(sink: &mut dyn MarkSink, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let Some(spatial) = ctx.space.spatial.as_ref() else {
        return;
    };
    let Some((lon_min, lat_min, lon_max, lat_max)) = spatial.geo_bounds() else {
        return;
    };

    let stroke = stroke_color(geo).unwrap_or_else(|| ctx.theme.grid_major_color.clone());
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, ctx.theme.grid_major_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let step = graticule_step(geo, lon_max - lon_min, lat_max - lat_min);

    let lat_lo = lat_min.max(-LAT_LIMIT);
    let lat_hi = lat_max.min(LAT_LIMIT);

    let mut d = String::new();
    // Meridians (constant longitude), west to east.
    for lon in axis_values(lon_min, lon_max, step) {
        append_line(spatial, &mut d, lat_lo, lat_hi, |t| (lon, t));
    }
    // Parallels (constant latitude), south to north.
    for lat in axis_values(lat_lo, lat_hi, step) {
        append_line(spatial, &mut d, lon_min, lon_max, |t| (t, lat));
    }

    if d.is_empty() {
        return;
    }
    sink.graticule_path(&d, &stroke, stroke_width, opacity_when_translucent(alpha));
}

/// Read a constant `stroke:` color setting, if present.
fn stroke_color(geo: &GeometryIr) -> Option<String> {
    geo.settings
        .iter()
        .find(|s| s.name == PropertyKey::Stroke)
        .and_then(|s| match &s.value {
            SettingValue::String(c) => Some(c.clone()),
            _ => None,
        })
}

/// The grid spacing in degrees: an explicit `step:` setting, else a "nice" value
/// chosen from the larger geographic span so a map shows roughly 4–10 lines.
fn graticule_step(geo: &GeometryIr, lon_span: f64, lat_span: f64) -> f64 {
    if let Some(setting) = geo.settings.iter().find(|s| s.name == PropertyKey::Step) {
        if let SettingValue::Number(n) = setting.value {
            if n > 0.0 {
                return n;
            }
        }
    }
    let target = lon_span.max(lat_span) / 8.0;
    const CANDIDATES: &[f64] = &[0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 45.0];
    CANDIDATES
        .iter()
        .copied()
        .find(|&c| c >= target)
        .unwrap_or(45.0)
}

/// The grid-line positions in `[min, max]` snapped to multiples of `step`.
fn axis_values(min: f64, max: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if step <= 0.0 || max <= min {
        return out;
    }
    let first = (min / step).ceil() * step;
    let mut v = first;
    // Guard against pathological step/extent combinations producing huge loops.
    while v <= max + f64::EPSILON && out.len() < 1024 {
        out.push(v);
        v += step;
    }
    out
}

/// Sample a grid line between `lo` and `hi` (the varying coordinate produced by
/// `point`) and append its projected subpath, breaking at unprojectable points.
fn append_line(
    spatial: &SpatialScale,
    d: &mut String,
    lo: f64,
    hi: f64,
    point: impl Fn(f64) -> (f64, f64),
) {
    const SEGMENTS: usize = 60;
    let mut started = false;
    for i in 0..=SEGMENTS {
        let t = lo + (hi - lo) * (i as f64 / SEGMENTS as f64);
        let (lon, lat) = point(t);
        match spatial.project_ll(lon, lat) {
            Some((x, y)) if started => {
                let _ = write!(d, "L{} {}", num(x), num(y));
            }
            Some((x, y)) => {
                let _ = write!(d, "M{} {}", num(x), num(y));
                started = true;
            }
            None => started = false,
        }
    }
}
