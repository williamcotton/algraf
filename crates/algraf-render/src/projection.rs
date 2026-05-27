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

/// Continental-US Albers equal-area (the `albers` alias and the lower-48
/// sub-projection of the `albers_usa` composite).
const ALBERS_LOWER48: &str = "+proj=aea +lat_1=29.5 +lat_2=45.5 +lat_0=37.5 \
     +lon_0=-96 +x_0=0 +y_0=0 +datum=WGS84 +units=m +no_defs";

/// Alaska conic equal-area, the `albers_usa` Alaska inset sub-projection. Centred
/// on Alaska (`lon_0=-154`, `lat_0=58.5`, parallels 55/65) following d3-geo's
/// `albersUsa` so the inset shape matches the conventional composite.
const ALBERS_ALASKA: &str = "+proj=aea +lat_1=55 +lat_2=65 +lat_0=58.5 \
     +lon_0=-154 +x_0=0 +y_0=0 +datum=WGS84 +units=m +no_defs";

/// Hawaii conic equal-area, the `albers_usa` Hawaii inset sub-projection.
const ALBERS_HAWAII: &str = "+proj=aea +lat_1=8 +lat_2=18 +lat_0=20 \
     +lon_0=-157 +x_0=0 +y_0=0 +datum=WGS84 +units=m +no_defs";

/// Approximate metres-per-radian scale of the Albers sub-projections, used to
/// place the Alaska/Hawaii insets relative to the lower-48 frame. The insets'
/// d3-geo offsets are fractions of the projection scale `k`; in `proj4rs` metre
/// space that scale is the WGS84 semi-major axis.
const ALBERS_K_METERS: f64 = 6_378_137.0;

/// Inset placement: `(longitude/latitude region test, sub-projection scale,
/// metre offset)` for Alaska and Hawaii, matching d3-geo's `albersUsa`. Offsets
/// are `(-dx·k, -dy·k)`; the latitude is flipped from d3's screen-down `+dy`
/// because this projected space is north-up (the SVG flip happens later in
/// [`SpatialScale::project_ll`]).
const ALASKA_SCALE: f64 = 0.35;
const ALASKA_OFFSET: (f64, f64) = (-0.307 * ALBERS_K_METERS, -0.201 * ALBERS_K_METERS);
const HAWAII_SCALE: f64 = 1.0;
const HAWAII_OFFSET: (f64, f64) = (-0.205 * ALBERS_K_METERS, -0.212 * ALBERS_K_METERS);

/// Maximum geographic span (degrees) of a line/ring segment before it is
/// resampled with intermediate points, so a long edge follows the projection's
/// curvature instead of a straight chord in pixel space (spec §16.15). Segments
/// shorter than this — typical detailed boundary data — are left untouched, so
/// existing maps render identically.
pub const MAX_SEGMENT_DEGREES: f64 = 5.0;

/// A longitude jump larger than this between consecutive vertices is treated as
/// an antimeridian crossing: the connecting chord is broken rather than drawn
/// across the whole map (spec §16.15).
pub const ANTIMERIDIAN_JUMP_DEGREES: f64 = 180.0;

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
    /// The `albers_usa` composite: lower-48 Albers plus Alaska and Hawaii insets,
    /// each a conic equal-area sub-projection scaled and translated into the
    /// lower-48 frame. Coordinates route to a sub-projection by geographic region
    /// (spec §16.14).
    AlbersUsa {
        from: Box<Proj>,
        lower48: Box<Proj>,
        alaska: Box<Proj>,
        hawaii: Box<Proj>,
    },
}

/// Which `albers_usa` sub-projection a `(lon, lat)` routes through. Regions are
/// disjoint geographic boxes so routing is deterministic (spec §16.14).
fn albers_region(lon: f64, lat: f64) -> AlbersRegion {
    // Hawaii: the main island chain, well south of the lower-48.
    if (16.0..26.0).contains(&lat) && (-165.0..-150.0).contains(&lon) {
        AlbersRegion::Hawaii
    } else if lat >= 50.0 {
        // Alaska, including the Aleutians (which reach ~52°N near and across the
        // antimeridian); the lower-48 never reaches 50°N.
        AlbersRegion::Alaska
    } else {
        AlbersRegion::Lower48
    }
}

#[derive(Clone, Copy)]
enum AlbersRegion {
    Lower48,
    Alaska,
    Hawaii,
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
            "albers" => ALBERS_LOWER48,
            // The `albersUsa`-style composite: lower-48 Albers with Alaska and
            // Hawaii insets routed by region (spec §16.14).
            "albers_usa" => return Self::albers_usa(),
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

    /// Build the `albers_usa` composite from its three sub-projections.
    fn albers_usa() -> Result<Projection, String> {
        let parse = |s: &str| Proj::from_proj_string(s).map_err(|e| format!("{e}"));
        Ok(Projection {
            kind: ProjKind::AlbersUsa {
                from: Box::new(parse(WGS84)?),
                lower48: Box::new(parse(ALBERS_LOWER48)?),
                alaska: Box::new(parse(ALBERS_ALASKA)?),
                hawaii: Box::new(parse(ALBERS_HAWAII)?),
            },
        })
    }

    /// Project WGS84 `(lon, lat)` in degrees to planar coordinates. Returns
    /// `None` for coordinates the projection cannot represent.
    pub fn project(&self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        match &self.kind {
            ProjKind::Equirectangular => Some((lon, lat)),
            ProjKind::Proj { from, to } => project_through(from, to, lon, lat),
            ProjKind::AlbersUsa {
                from,
                lower48,
                alaska,
                hawaii,
            } => {
                let (to, scale, offset) = match albers_region(lon, lat) {
                    AlbersRegion::Lower48 => (lower48, 1.0, (0.0, 0.0)),
                    AlbersRegion::Alaska => (alaska, ALASKA_SCALE, ALASKA_OFFSET),
                    AlbersRegion::Hawaii => (hawaii, HAWAII_SCALE, HAWAII_OFFSET),
                };
                let (x, y) = project_through(from, to, lon, lat)?;
                Some((x * scale + offset.0, y * scale + offset.1))
            }
        }
    }
}

/// Transform a WGS84 `(lon, lat)` in degrees through a `proj4rs` source→target
/// pair, returning `None` for coordinates the projection cannot represent.
fn project_through(from: &Proj, to: &Proj, lon: f64, lat: f64) -> Option<(f64, f64)> {
    let mut point = (lon.to_radians(), lat.to_radians(), 0.0_f64);
    proj4rs::transform::transform(from, to, &mut point).ok()?;
    (point.0.is_finite() && point.1.is_finite()).then_some((point.0, point.1))
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
    /// The geographic bounding box `(lon_min, lat_min, lon_max, lat_max)` of the
    /// rendered data, so a `Graticule` knows which meridians/parallels to draw.
    geo_bounds: Option<(f64, f64, f64, f64)>,
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
            geo_bounds: None,
        }
    }

    /// Record the geographic bounding box `(lon_min, lat_min, lon_max, lat_max)`
    /// of the rendered data, consumed by the `Graticule` mark.
    pub fn set_geo_bounds(&mut self, bounds: (f64, f64, f64, f64)) {
        self.geo_bounds = Some(bounds);
    }

    /// The geographic bounding box of the rendered data, if known.
    pub fn geo_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        self.geo_bounds
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

/// Whether the longitude step between two consecutive vertices crosses the
/// antimeridian (spec §16.15), so the connecting chord should be broken.
pub fn is_antimeridian_jump(lon_a: f64, lon_b: f64) -> bool {
    (lon_b - lon_a).abs() > ANTIMERIDIAN_JUMP_DEGREES
}

/// Append intermediate geographic samples between `a` and `b` so no sub-segment
/// exceeds [`MAX_SEGMENT_DEGREES`]; the start point is excluded and the end
/// point included. A short segment yields exactly one sample (its endpoint), so
/// detailed boundary data is projected vertex-for-vertex as before.
pub fn resample_segment(a: (f64, f64), b: (f64, f64), out: &mut Vec<(f64, f64)>) {
    let span = (b.0 - a.0).abs().max((b.1 - a.1).abs());
    let steps = (span / MAX_SEGMENT_DEGREES).ceil().max(1.0) as usize;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn albers_usa_routes_each_region() {
        let proj = Projection::resolve(Some("albers_usa")).expect("albers_usa builds");
        // A lower-48 point near the projection centre lands near the origin.
        let (lx, ly) = proj.project(-96.0, 37.5).expect("lower-48 projects");
        assert!(
            lx.abs() < 1.0e5 && ly.abs() < 1.0e5,
            "lower-48 ({lx}, {ly})"
        );
        // Alaska routes through the lower-left inset (left of and below centre).
        let (ax, ay) = proj.project(-150.0, 63.0).expect("alaska projects");
        assert!(ax < lx, "alaska x {ax} should sit left of lower-48 {lx}");
        assert!(ay < ly, "alaska y {ay} should sit below lower-48 {ly}");
        // Hawaii routes through its own inset, also below the lower-48.
        let (hx, hy) = proj.project(-157.0, 20.0).expect("hawaii projects");
        assert!(hy < ly, "hawaii y {hy} should sit below lower-48 {ly}");
        assert!(hx < lx, "hawaii x {hx} should sit left of lower-48 {lx}");
    }

    #[test]
    fn albers_usa_lower48_matches_plain_albers() {
        // For lower-48 coordinates the composite is the plain `albers` Albers, so
        // existing lower-48 maps keep their appearance (spec §16.14).
        let composite = Projection::resolve(Some("albers_usa")).unwrap();
        let albers = Projection::resolve(Some("albers")).unwrap();
        for (lon, lat) in [(-122.4, 37.8), (-74.0, 40.7), (-87.6, 41.9)] {
            let c = composite.project(lon, lat).unwrap();
            let a = albers.project(lon, lat).unwrap();
            assert!((c.0 - a.0).abs() < 1.0e-6 && (c.1 - a.1).abs() < 1.0e-6);
        }
    }

    #[test]
    fn alaska_inset_is_scaled_down() {
        // The Alaska inset is ~0.35× scale, so a degree of Alaska spans far less
        // projected distance than a degree of the lower-48.
        let proj = Projection::resolve(Some("albers_usa")).unwrap();
        let ak_a = proj.project(-150.0, 63.0).unwrap();
        let ak_b = proj.project(-149.0, 63.0).unwrap();
        let ak_span = (ak_a.0 - ak_b.0).hypot(ak_a.1 - ak_b.1);
        let l48_a = proj.project(-96.0, 39.0).unwrap();
        let l48_b = proj.project(-95.0, 39.0).unwrap();
        let l48_span = (l48_a.0 - l48_b.0).hypot(l48_a.1 - l48_b.1);
        assert!(
            ak_span < l48_span,
            "alaska {ak_span} vs lower-48 {l48_span}"
        );
    }
}
