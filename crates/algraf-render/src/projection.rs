//! Cartographic projection and the spatial scale (spec §16.14, §16.15).
//!
//! A spatial space maps geographic coordinates (WGS84 lon/lat) → projected
//! coordinates → pixels. Projection is resolved from a friendly alias or a raw
//! `+proj=…` PROJ string through [`proj4rs`]; the [`SpatialScale`] then fits the
//! projected bounding box into the plot rectangle **preserving aspect ratio**
//! (letterboxing), so equal-area maps are never stretched.

use geo_types::{Coord, Geometry};
use proj4rs::Proj;

use crate::layout::Rect;

/// The default source CRS for all spatial data: WGS84 geographic (lon/lat).
const WGS84: &str = "+proj=longlat +datum=WGS84 +no_defs";

/// Continental-US Albers equal-area (the lower-48 sub-case of `albers_usa`).
/// `albers_usa`'s conventional Alaska/Hawaii insets are deferred; the checked-in
/// county fixture is lower-48 + DC, so the composite degrades to this single
/// projection (see V0_8_PLAN Must #5).
const ALBERS_LOWER48: &str = "+proj=aea +lat_1=29.5 +lat_2=45.5 +lat_0=37.5 \
     +lon_0=-96 +x_0=0 +y_0=0 +datum=WGS84 +units=m +no_defs";

/// A resolved projection from WGS84 lon/lat into a planar coordinate system.
pub struct Projection {
    kind: ProjKind,
}

enum ProjKind {
    /// Equirectangular (plate carrée): planar lon/lat in degrees, the default
    /// when no projection is named so raw long/lat maps degrade gracefully.
    Equirectangular,
    /// A `proj4rs` target CRS; input arrives as WGS84 lon/lat. Boxed because a
    /// `Proj` is large relative to the equirectangular variant.
    Proj { from: Box<Proj>, to: Box<Proj> },
}

impl Projection {
    /// Resolve a projection from its alias or raw PROJ string. `None` selects
    /// the default equirectangular projection (spec §16.14). An unknown alias or
    /// malformed PROJ string is an error (`E1802`).
    pub fn resolve(name: Option<&str>) -> Result<Projection, String> {
        let Some(name) = name else {
            return Ok(Projection {
                kind: ProjKind::Equirectangular,
            });
        };
        let proj_string = match name {
            "equirectangular" => {
                return Ok(Projection {
                    kind: ProjKind::Equirectangular,
                })
            }
            "mercator" => "+proj=merc +datum=WGS84 +no_defs",
            "robinson" => "+proj=robin +datum=WGS84 +no_defs",
            // `albers` (lower-48) and `albers_usa` share the continental Albers
            // here; AK/HI insets are deferred.
            "albers" | "albers_usa" => ALBERS_LOWER48,
            raw if raw.starts_with("+proj=") => raw,
            other => return Err(format!("unknown projection `{other}`")),
        };
        let from = Proj::from_proj_string(WGS84).map_err(|e| format!("{e}"))?;
        let to = Proj::from_proj_string(proj_string).map_err(|e| format!("{e}"))?;
        Ok(Projection {
            kind: ProjKind::Proj {
                from: Box::new(from),
                to: Box::new(to),
            },
        })
    }

    /// Project WGS84 `(lon, lat)` in degrees to planar coordinates. Returns
    /// `None` for coordinates the projection cannot represent.
    pub fn project(&self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        match &self.kind {
            ProjKind::Equirectangular => Some((lon, lat)),
            ProjKind::Proj { from, to } => {
                let mut point = (lon.to_radians(), lat.to_radians(), 0.0_f64);
                proj4rs::transform::transform(from.as_ref(), to.as_ref(), &mut point).ok()?;
                (point.0.is_finite() && point.1.is_finite()).then_some((point.0, point.1))
            }
        }
    }
}

/// A trained spatial scale: a projection plus an aspect-preserving fit of the
/// projected bounding box into the plot rectangle (spec §16.15). Maps geographic
/// (lon, lat) → projected → pixel, replacing the planar x/y scales for a spatial
/// space.
pub struct SpatialScale {
    projection: Projection,
    min_x: f64,
    max_y: f64,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    /// For a projected `long * lat` overlay space, the longitude/latitude column
    /// names so point/line marks resolve their position through the projection.
    pub lon_col: Option<String>,
    pub lat_col: Option<String>,
    /// For a `Space(geom)` space, the geometry column the `Geo` mark walks.
    pub geom_col: Option<String>,
}

impl SpatialScale {
    /// Fit a projected bounding box `(min_x, min_y, max_x, max_y)` into the plot
    /// rectangle, preserving aspect ratio (letterbox).
    pub fn fit(projection: Projection, bbox: (f64, f64, f64, f64), plot: Rect) -> SpatialScale {
        let (min_x, min_y, max_x, max_y) = bbox;
        let span_x = (max_x - min_x).max(f64::MIN_POSITIVE);
        let span_y = (max_y - min_y).max(f64::MIN_POSITIVE);
        let scale = (plot.width / span_x).min(plot.height / span_y);
        let used_w = span_x * scale;
        let used_h = span_y * scale;
        SpatialScale {
            projection,
            min_x,
            max_y,
            scale,
            offset_x: plot.x + (plot.width - used_w) / 2.0,
            offset_y: plot.y + (plot.height - used_h) / 2.0,
            lon_col: None,
            lat_col: None,
            geom_col: None,
        }
    }

    /// Map geographic `(lon, lat)` to a pixel coordinate, or `None` if the
    /// projection rejects it. The projected y axis is flipped for SVG.
    pub fn project_ll(&self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        let (x, y) = self.projection.project(lon, lat)?;
        Some((
            self.offset_x + (x - self.min_x) * self.scale,
            self.offset_y + (self.max_y - y) * self.scale,
        ))
    }
}

/// Visit every `(lon, lat)` coordinate of a geometry in source order, so the
/// projected bounding box and the `Geo` renderer walk vertices identically
/// (spec §14.x determinism).
pub fn for_each_coord(geometry: &Geometry<f64>, f: &mut impl FnMut(f64, f64)) {
    let mut coord = |c: &Coord<f64>| f(c.x, c.y);
    match geometry {
        Geometry::Point(p) => coord(&p.0),
        Geometry::Line(l) => {
            coord(&l.start);
            coord(&l.end);
        }
        Geometry::LineString(ls) => ls.0.iter().for_each(&mut coord),
        Geometry::Polygon(p) => {
            p.exterior().0.iter().for_each(&mut coord);
            for ring in p.interiors() {
                ring.0.iter().for_each(&mut coord);
            }
        }
        Geometry::MultiPoint(mp) => mp.0.iter().for_each(|p| coord(&p.0)),
        Geometry::MultiLineString(mls) => {
            mls.0.iter().for_each(|ls| ls.0.iter().for_each(&mut coord))
        }
        Geometry::MultiPolygon(mp) => {
            for poly in &mp.0 {
                poly.exterior().0.iter().for_each(&mut coord);
                for ring in poly.interiors() {
                    ring.0.iter().for_each(&mut coord);
                }
            }
        }
        Geometry::GeometryCollection(gc) => {
            for g in &gc.0 {
                for_each_coord(g, f);
            }
        }
        Geometry::Rect(r) => {
            let r = r.to_polygon();
            r.exterior().0.iter().for_each(|c| f(c.x, c.y));
        }
        Geometry::Triangle(t) => t.to_array().iter().for_each(|c| f(c.x, c.y)),
    }
}
