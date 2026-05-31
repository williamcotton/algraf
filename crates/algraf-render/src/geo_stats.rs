//! Geometry-producing spatial stats (spec §15.13).
//!
//! `Centroid(geom)` and `Simplify(geom, tolerance: …)` consume a geometry column
//! and produce a derived table whose geometry column is replaced by the computed
//! geometry, with every scalar column passed through unchanged. Both are pure
//! and deterministic — they read only the input table through [`Table`] and
//! depend on no external resources — so they materialize like any other derived
//! table (spec §15.3) and render through the `Geo` mark.

use algraf_data::geo_types::{
    Coord, Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};
use algraf_data::{Column, ColumnDef, DataFrame, DataType, DataValueRef, Table};
use algraf_semantics::{geometry_column_name, spatial_join_appended_columns};

/// Compute the centroid of each row's geometry, passing scalar columns through.
pub fn centroid(table: &dyn Table, geom_col: &str) -> DataFrame {
    geometry_map(table, geom_col, |g| centroid_point(g).map(Geometry::Point))
}

/// Simplify each row's geometry with Douglas–Peucker at `tolerance` (in the
/// geometry's own coordinate units, i.e. degrees for WGS84), passing scalar
/// columns through.
pub fn simplify(table: &dyn Table, geom_col: &str, tolerance: f64) -> DataFrame {
    let tol = tolerance.max(0.0);
    geometry_map(table, geom_col, |g| Some(simplify_geometry(g, tol)))
}

/// Build a derived frame replacing the `geom_col` geometry with `f(geometry)`
/// and copying every other column through unchanged (spec §15.13).
fn geometry_map(
    table: &dyn Table,
    geom_col: &str,
    f: impl Fn(&Geometry<f64>) -> Option<Geometry<f64>>,
) -> DataFrame {
    let rows = table.row_count();
    let mut schema = Vec::with_capacity(table.schema().len());
    let mut columns = Vec::with_capacity(table.schema().len());
    for def in table.schema() {
        if def.name == geom_col {
            let geoms: Vec<Option<Geometry<f64>>> = (0..rows)
                .map(|row| match table.value(&def.name, row) {
                    Some(DataValueRef::Geometry(g)) => f(g),
                    _ => None,
                })
                .collect();
            schema.push(ColumnDef {
                name: def.name.clone(),
                dtype: DataType::Geometry,
                nullable: geoms.iter().any(Option::is_none),
                examples: Vec::new(),
            });
            columns.push(Column::Geometry(geoms));
        } else {
            schema.push(def.clone());
            columns.push(passthrough_column(table, def, rows));
        }
    }
    DataFrame::new(schema, columns)
}

/// Copy a non-geometry column out through the [`Table`] boundary, rebuilding the
/// typed [`Column`] that matches its declared dtype.
fn passthrough_column(table: &dyn Table, def: &ColumnDef, rows: usize) -> Column {
    let cell = |row: usize| table.value(&def.name, row);
    match def.dtype {
        DataType::Boolean => Column::from_bool_options(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::Bool(b)) => Some(b),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Integer => Column::from_int_options(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::Int(i)) => Some(i),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Float => Column::from_float_options(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::Float(f)) => Some(f),
                    Some(DataValueRef::Int(i)) => Some(i as f64),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Temporal => Column::from_temporal_options(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::Temporal(t)) => Some(t),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Geometry => Column::Geometry(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::Geometry(g)) => Some(g.clone()),
                    _ => None,
                })
                .collect(),
        ),
        // String, Mixed, and Unknown all back onto string storage (spec §10.3).
        DataType::String | DataType::Mixed | DataType::Unknown => Column::String(
            (0..rows)
                .map(|r| match cell(r) {
                    Some(DataValueRef::String(s)) => Some(s.to_string()),
                    _ => None,
                })
                .collect(),
        ),
    }
}

// --- Spatial join -----------------------------------------------------------

/// Join `point_table` (point geometries in `point_geom_col`) against
/// `polygon_table` by the `within` predicate, appending the polygon table's
/// scalar columns to each matching point row (spec §15.14). When a point matches
/// several polygons, the first in polygon-row order wins; a point with no match
/// or no geometry gets missing cells. The point side passes through unchanged.
pub fn spatial_join_within(
    point_table: &dyn Table,
    point_geom_col: &str,
    polygon_table: &dyn Table,
) -> DataFrame {
    let rows = point_table.row_count();

    // Locate the polygon side's geometry column and the columns to append.
    let polygon_geom = geometry_column_name(
        polygon_table
            .schema()
            .iter()
            .map(|c| (c.name.as_str(), c.dtype)),
    );
    let point_names: Vec<&str> = point_table
        .schema()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    let appended = spatial_join_appended_columns(
        point_names,
        polygon_table
            .schema()
            .iter()
            .map(|c| (c.name.as_str(), c.dtype)),
    );

    // For each point, the first containing polygon row (in row order).
    let matched: Vec<Option<usize>> = (0..rows)
        .map(|row| {
            match_polygon(
                point_table,
                point_geom_col,
                row,
                polygon_table,
                polygon_geom.as_deref(),
            )
        })
        .collect();

    let mut schema = Vec::new();
    let mut columns = Vec::new();
    // Point side, unchanged.
    for def in point_table.schema() {
        schema.push(def.clone());
        columns.push(passthrough_column(point_table, def, rows));
    }
    // Polygon side, selected per matched row.
    for col in &appended {
        schema.push(ColumnDef {
            name: col.name.clone(),
            dtype: col.dtype,
            nullable: true,
            examples: Vec::new(),
        });
        columns.push(select_column(polygon_table, &col.name, col.dtype, &matched));
    }
    DataFrame::new(schema, columns)
}

/// The first polygon row containing the point at `row`, in polygon-row order.
fn match_polygon(
    point_table: &dyn Table,
    point_geom_col: &str,
    row: usize,
    polygon_table: &dyn Table,
    polygon_geom: Option<&str>,
) -> Option<usize> {
    let polygon_geom = polygon_geom?;
    let (x, y) = match point_table.value(point_geom_col, row) {
        Some(DataValueRef::Geometry(g)) => centroid_point(g).map(|p| (p.x(), p.y()))?,
        _ => return None,
    };
    (0..polygon_table.row_count()).find(|&p| {
        matches!(
            polygon_table.value(polygon_geom, p),
            Some(DataValueRef::Geometry(g)) if geometry_contains_point(g, x, y)
        )
    })
}

/// Build an appended column by reading each point's matched polygon row.
fn select_column(
    polygon_table: &dyn Table,
    name: &str,
    dtype: DataType,
    matched: &[Option<usize>],
) -> Column {
    let cell = |row: Option<usize>| row.and_then(|r| polygon_table.value(name, r));
    match dtype {
        DataType::Boolean => Column::from_bool_options(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::Bool(b)) => Some(b),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Integer => Column::from_int_options(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::Int(i)) => Some(i),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Float => Column::from_float_options(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::Float(f)) => Some(f),
                    Some(DataValueRef::Int(i)) => Some(i as f64),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Temporal => Column::from_temporal_options(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::Temporal(t)) => Some(t),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Geometry => Column::Geometry(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::Geometry(g)) => Some(g.clone()),
                    _ => None,
                })
                .collect(),
        ),
        DataType::String | DataType::Mixed | DataType::Unknown => Column::String(
            matched
                .iter()
                .map(|&r| match cell(r) {
                    Some(DataValueRef::String(s)) => Some(s.to_string()),
                    _ => None,
                })
                .collect(),
        ),
    }
}

/// Whether a point lies inside an areal geometry (even-odd ray casting, holes
/// excluded). Non-areal geometries never contain a point.
fn geometry_contains_point(geometry: &Geometry<f64>, x: f64, y: f64) -> bool {
    match geometry {
        Geometry::Polygon(p) => polygon_contains(p, x, y),
        Geometry::MultiPolygon(MultiPolygon(polys)) => {
            polys.iter().any(|p| polygon_contains(p, x, y))
        }
        Geometry::Rect(r) => polygon_contains(&r.to_polygon(), x, y),
        Geometry::Triangle(t) => polygon_contains(&t.to_polygon(), x, y),
        Geometry::GeometryCollection(gc) => gc.0.iter().any(|g| geometry_contains_point(g, x, y)),
        _ => false,
    }
}

fn polygon_contains(poly: &Polygon<f64>, x: f64, y: f64) -> bool {
    ring_contains(poly.exterior(), x, y)
        && !poly
            .interiors()
            .iter()
            .any(|ring| ring_contains(ring, x, y))
}

/// Even-odd ray-casting point-in-ring test.
fn ring_contains(ring: &LineString<f64>, x: f64, y: f64) -> bool {
    let pts = &ring.0;
    let mut inside = false;
    for w in pts.windows(2) {
        let (xi, yi) = (w[0].x, w[0].y);
        let (xj, yj) = (w[1].x, w[1].y);
        let intersects = (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi;
        if intersects {
            inside = !inside;
        }
    }
    inside
}

// --- Centroid ---------------------------------------------------------------

/// The centroid of a geometry: the area-weighted centroid for areal geometries,
/// the mean vertex otherwise. Returns `None` for an empty geometry.
fn centroid_point(geometry: &Geometry<f64>) -> Option<Point<f64>> {
    match geometry {
        Geometry::Point(p) => Some(*p),
        Geometry::MultiPoint(MultiPoint(points)) => mean_point(points.iter().map(|p| p.0)),
        Geometry::Line(l) => Some(Point::new(
            (l.start.x + l.end.x) / 2.0,
            (l.start.y + l.end.y) / 2.0,
        )),
        Geometry::LineString(ls) => mean_point(ls.0.iter().copied()),
        Geometry::MultiLineString(MultiLineString(lines)) => {
            mean_point(lines.iter().flat_map(|ls| ls.0.iter().copied()))
        }
        Geometry::Polygon(poly) => polygon_centroid(poly),
        Geometry::MultiPolygon(MultiPolygon(polys)) => multi_polygon_centroid(polys),
        Geometry::Rect(r) => polygon_centroid(&r.to_polygon()),
        Geometry::Triangle(t) => polygon_centroid(&t.to_polygon()),
        Geometry::GeometryCollection(gc) => {
            mean_point(gc.0.iter().filter_map(|g| centroid_point(g).map(|p| p.0)))
        }
    }
}

fn mean_point(coords: impl Iterator<Item = Coord<f64>>) -> Option<Point<f64>> {
    let (sum_x, sum_y, n) = coords.fold((0.0, 0.0, 0usize), |(sx, sy, n), c| {
        (sx + c.x, sy + c.y, n + 1)
    });
    (n > 0).then(|| Point::new(sum_x / n as f64, sum_y / n as f64))
}

/// Area-weighted centroid of a polygon's exterior ring (the standard shoelace
/// centroid). Falls back to the mean vertex for a degenerate (zero-area) ring.
fn polygon_centroid(poly: &Polygon<f64>) -> Option<Point<f64>> {
    ring_centroid(poly.exterior()).or_else(|| mean_point(poly.exterior().0.iter().copied()))
}

fn ring_centroid(ring: &LineString<f64>) -> Option<Point<f64>> {
    let pts = &ring.0;
    if pts.len() < 3 {
        return None;
    }
    let mut area2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for w in pts.windows(2) {
        let cross = w[0].x * w[1].y - w[1].x * w[0].y;
        area2 += cross;
        cx += (w[0].x + w[1].x) * cross;
        cy += (w[0].y + w[1].y) * cross;
    }
    if area2.abs() < f64::EPSILON {
        return None;
    }
    Some(Point::new(cx / (3.0 * area2), cy / (3.0 * area2)))
}

fn multi_polygon_centroid(polys: &[Polygon<f64>]) -> Option<Point<f64>> {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut total = 0.0;
    for poly in polys {
        if let Some(c) = polygon_centroid(poly) {
            let w = ring_area(poly.exterior()).abs().max(f64::MIN_POSITIVE);
            sum_x += c.x() * w;
            sum_y += c.y() * w;
            total += w;
        }
    }
    (total > 0.0).then(|| Point::new(sum_x / total, sum_y / total))
}

fn ring_area(ring: &LineString<f64>) -> f64 {
    ring.0
        .windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum::<f64>()
        / 2.0
}

// --- Simplify (Douglas–Peucker) ---------------------------------------------

fn simplify_geometry(geometry: &Geometry<f64>, tol: f64) -> Geometry<f64> {
    match geometry {
        Geometry::LineString(ls) => Geometry::LineString(LineString(dp(&ls.0, tol, false))),
        Geometry::MultiLineString(MultiLineString(lines)) => {
            Geometry::MultiLineString(MultiLineString(
                lines
                    .iter()
                    .map(|ls| LineString(dp(&ls.0, tol, false)))
                    .collect(),
            ))
        }
        Geometry::Polygon(poly) => Geometry::Polygon(simplify_polygon(poly, tol)),
        Geometry::MultiPolygon(MultiPolygon(polys)) => Geometry::MultiPolygon(MultiPolygon(
            polys.iter().map(|p| simplify_polygon(p, tol)).collect(),
        )),
        // Points and other primitives are returned unchanged.
        other => other.clone(),
    }
}

fn simplify_polygon(poly: &Polygon<f64>, tol: f64) -> Polygon<f64> {
    let exterior = LineString(dp(&poly.exterior().0, tol, true));
    let interiors = poly
        .interiors()
        .iter()
        .map(|ring| LineString(dp(&ring.0, tol, true)))
        .collect::<Vec<_>>();
    Polygon::new(exterior, interiors)
}

/// Douglas–Peucker simplification. When `closed`, a ring that simplifies below 4
/// points (3 distinct + closing point) is returned unchanged so it stays a valid
/// polygon ring.
fn dp(points: &[Coord<f64>], tol: f64, closed: bool) -> Vec<Coord<f64>> {
    if points.len() <= 2 || tol <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    dp_recurse(points, 0, points.len() - 1, tol, &mut keep);
    let simplified: Vec<Coord<f64>> = points
        .iter()
        .zip(&keep)
        .filter_map(|(c, &k)| k.then_some(*c))
        .collect();
    if closed && simplified.len() < 4 {
        points.to_vec()
    } else {
        simplified
    }
}

fn dp_recurse(points: &[Coord<f64>], first: usize, last: usize, tol: f64, keep: &mut [bool]) {
    if last <= first + 1 {
        return;
    }
    let (a, b) = (points[first], points[last]);
    let mut max_dist = 0.0;
    let mut index = first;
    for (i, p) in points.iter().enumerate().take(last).skip(first + 1) {
        let dist = perpendicular_distance(*p, a, b);
        if dist > max_dist {
            max_dist = dist;
            index = i;
        }
    }
    if max_dist > tol {
        keep[index] = true;
        dp_recurse(points, first, index, tol, keep);
        dp_recurse(points, index, last, tol, keep);
    }
}

fn perpendicular_distance(p: Coord<f64>, a: Coord<f64>, b: Coord<f64>) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.hypot(dy);
    if len < f64::EPSILON {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
}

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_data::geo_types::{Coord, LineString, Polygon};

    fn square() -> Geometry<f64> {
        Geometry::Polygon(Polygon::new(
            LineString(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 10.0, y: 10.0 },
                Coord { x: 0.0, y: 10.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        ))
    }

    #[test]
    fn square_centroid_is_its_middle() {
        let c = centroid_point(&square()).unwrap();
        assert!((c.x() - 5.0).abs() < 1e-9 && (c.y() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn collinear_points_simplify_to_endpoints() {
        let line = vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 1.0, y: 0.0 },
            Coord { x: 2.0, y: 0.0 },
            Coord { x: 3.0, y: 0.0 },
        ];
        let out = dp(&line, 0.01, false);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn closed_ring_keeps_validity_when_oversimplified() {
        let ring = square();
        let Geometry::Polygon(p) = simplify_geometry(&ring, 1000.0) else {
            panic!("expected polygon");
        };
        // A huge tolerance would drop the ring below 4 points, so it is kept whole.
        assert!(p.exterior().0.len() >= 4);
    }
}
