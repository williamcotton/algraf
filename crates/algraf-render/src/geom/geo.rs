use std::fmt::Write;

use algraf_data::geo_types::{Geometry, LineString};
use algraf_data::DataValueRef;
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{opacity_attr, render_rows, DEFAULT_FILL};
use super::GeometryRenderContext;

// --- Geo: the polymorphic spatial mark (spec §14.x, §16.15) -----------------

/// Default marker radius (px) for a `Geo` point feature.
const GEO_POINT_RADIUS: f64 = 2.5;

/// Render a `Geo` layer: walk each row's geometry, project every coordinate
/// through the spatial scale, and dispatch on geometry type — Point→circle,
/// LineString→polyline, Polygon/MultiPolygon→path (even-odd fill). Features are
/// drawn in row order and rings in source order, so output is deterministic
/// (spec §14.x).
pub(super) fn render(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(spatial) = space.spatial.as_ref() else {
        return;
    };
    let Some(geom_col) = spatial.geom_col.clone() else {
        return;
    };
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 0.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);

    for row in render_rows(table, rows) {
        let Some(DataValueRef::Geometry(geometry)) = table.value(&geom_col, row) else {
            continue;
        };
        let fill_color = fill.resolve(table, row);
        let stroke_color = stroke.resolve(table, row);
        emit_geometry(
            w,
            geometry,
            spatial,
            fill_color.as_deref(),
            stroke_color.as_deref(),
            stroke_width,
            alpha,
        );
    }
}

fn emit_geometry(
    w: &mut SvgWriter,
    geometry: &Geometry<f64>,
    spatial: &crate::projection::SpatialScale,
    fill: Option<&str>,
    stroke: Option<&str>,
    stroke_width: f64,
    alpha: f64,
) {
    match geometry {
        Geometry::Point(p) => emit_geo_point(w, spatial, p.x(), p.y(), fill, alpha),
        Geometry::MultiPoint(mp) => {
            for p in &mp.0 {
                emit_geo_point(w, spatial, p.x(), p.y(), fill, alpha);
            }
        }
        Geometry::Line(l) => {
            let d = path_from_rings(spatial, [&LineString::from(vec![l.start, l.end])], false);
            emit_geo_path(w, &d, None, stroke, stroke_width, alpha, false);
        }
        Geometry::LineString(ls) => {
            let d = path_from_rings(spatial, [ls], false);
            emit_geo_path(w, &d, None, stroke, stroke_width, alpha, false);
        }
        Geometry::MultiLineString(mls) => {
            let d = path_from_rings(spatial, mls.0.iter(), false);
            emit_geo_path(w, &d, None, stroke, stroke_width, alpha, false);
        }
        Geometry::Polygon(poly) => {
            let rings = std::iter::once(poly.exterior()).chain(poly.interiors());
            let d = path_from_rings(spatial, rings, true);
            emit_geo_path(w, &d, fill, stroke, stroke_width, alpha, true);
        }
        Geometry::MultiPolygon(mp) => {
            let mut d = String::new();
            for poly in &mp.0 {
                for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
                    append_ring(spatial, ring, true, &mut d);
                }
            }
            emit_geo_path(w, &d, fill, stroke, stroke_width, alpha, true);
        }
        // Rect/Triangle/GeometryCollection are uncommon in feature data; outline
        // them as polygons via their boundary.
        Geometry::Rect(r) => {
            let poly = r.to_polygon();
            let d = path_from_rings(spatial, [poly.exterior()], true);
            emit_geo_path(w, &d, fill, stroke, stroke_width, alpha, true);
        }
        Geometry::Triangle(t) => {
            let poly = t.to_polygon();
            let d = path_from_rings(spatial, [poly.exterior()], true);
            emit_geo_path(w, &d, fill, stroke, stroke_width, alpha, true);
        }
        Geometry::GeometryCollection(gc) => {
            for g in &gc.0 {
                emit_geometry(w, g, spatial, fill, stroke, stroke_width, alpha);
            }
        }
    }
}

/// Build an SVG path `d` from rings, projecting each coordinate. `close`
/// appends `Z` per ring (for areal geometries).
fn path_from_rings<'a>(
    spatial: &crate::projection::SpatialScale,
    rings: impl IntoIterator<Item = &'a LineString<f64>>,
    close: bool,
) -> String {
    let mut d = String::new();
    for ring in rings {
        append_ring(spatial, ring, close, &mut d);
    }
    d
}

/// Append a projected ring/line subpath, resampling long segments so curved
/// projections follow the projection and breaking the line at antimeridian
/// crossings instead of drawing a chord across the map (spec §16.15).
fn append_ring(
    spatial: &crate::projection::SpatialScale,
    ring: &LineString<f64>,
    close: bool,
    d: &mut String,
) {
    let mut started = false;
    let mut samples: Vec<(f64, f64)> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    let emit = |lon: f64, lat: f64, started: &mut bool, d: &mut String| {
        match spatial.project_ll(lon, lat) {
            Some((x, y)) if *started => {
                let _ = write!(d, "L{} {}", num(x), num(y));
            }
            Some((x, y)) => {
                let _ = write!(d, "M{} {}", num(x), num(y));
                *started = true;
            }
            // An unprojectable point breaks the line; the next point restarts it.
            None => *started = false,
        }
    };
    for c in &ring.0 {
        let cur = (c.x, c.y);
        match prev {
            None => emit(cur.0, cur.1, &mut started, d),
            Some(p) if crate::projection::is_antimeridian_jump(p.0, cur.0) => {
                // Break the chord across the antimeridian: start a fresh subpath.
                started = false;
                emit(cur.0, cur.1, &mut started, d);
            }
            Some(p) => {
                samples.clear();
                crate::projection::resample_segment(p, cur, &mut samples);
                for &(lon, lat) in &samples {
                    emit(lon, lat, &mut started, d);
                }
            }
        }
        prev = Some(cur);
    }
    if started && close {
        d.push('Z');
    }
}

fn emit_geo_point(
    w: &mut SvgWriter,
    spatial: &crate::projection::SpatialScale,
    lon: f64,
    lat: f64,
    fill: Option<&str>,
    alpha: f64,
) {
    let Some((cx, cy)) = spatial.project_ll(lon, lat) else {
        return;
    };
    let fill = fill.unwrap_or(DEFAULT_FILL);
    w.line(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"{}/>",
        num(cx),
        num(cy),
        num(GEO_POINT_RADIUS),
        escape_attr(fill),
        opacity_attr(alpha),
    ));
}

fn emit_geo_path(
    w: &mut SvgWriter,
    d: &str,
    fill: Option<&str>,
    stroke: Option<&str>,
    stroke_width: f64,
    alpha: f64,
    areal: bool,
) {
    if d.is_empty() {
        return;
    }
    let fill_attr = if areal {
        format!(" fill=\"{}\"", escape_attr(fill.unwrap_or(DEFAULT_FILL)))
    } else {
        " fill=\"none\"".to_string()
    };
    let fill_rule = if areal { " fill-rule=\"evenodd\"" } else { "" };
    let stroke_attr = match stroke {
        Some(color) => format!(
            " stroke=\"{}\" stroke-width=\"{}\"",
            escape_attr(color),
            num(stroke_width.max(0.0)),
        ),
        None => String::new(),
    };
    w.line(&format!(
        "<path d=\"{}\"{}{}{}{}/>",
        d,
        fill_attr,
        fill_rule,
        stroke_attr,
        opacity_attr(alpha),
    ));
}
