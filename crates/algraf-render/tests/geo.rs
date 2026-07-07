//! Spatial rendering: projection, spatial scale, and the `Geo` mark
//! (spec §14.x, §16.14, §16.15, §27.1).

use std::collections::HashMap;

use algraf_data::{read_geojson_str, DataFrame, Table};
use algraf_render::{render, RenderOptions, RenderResult, Theme};
use algraf_semantics::{analyze, analyze_with_tables};
use algraf_syntax::parse;

/// Parse + analyze + render `source` against an inline GeoJSON FeatureCollection.
fn render_geojson(source: &str, geojson: &str) -> RenderResult {
    let frame = read_geojson_str(geojson).expect("geojson").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    render(&ir, &frame, &Theme::void(), None).expect("render")
}

/// Render against a primary GeoJSON plus one named GeoJSON table.
fn render_geojson_with_table(
    source: &str,
    primary: &str,
    table_name: &str,
    table_geojson: &str,
) -> RenderResult {
    let primary_frame = read_geojson_str(primary).expect("primary").frame;
    let table_frame = read_geojson_str(table_geojson).expect("table").frame;
    let mut schemas = HashMap::new();
    schemas.insert(table_name.to_string(), table_frame.schema().to_vec());
    let mut frames: HashMap<String, DataFrame> = HashMap::new();
    frames.insert(table_name.to_string(), table_frame);
    let parsed = parse(source);
    let analysis = analyze_with_tables(&parsed.syntax(), primary_frame.schema(), &schemas);
    let ir = analysis.ir.expect("ir");
    render(
        &ir,
        &primary_frame,
        &Theme::void(),
        RenderOptions::default().with_named_tables(&frames),
    )
    .expect("render")
}

/// A FeatureCollection with a polygon-with-hole and a multipolygon, each with a
/// `population` value, so the choropleth fill differs per feature.
const POLYGONS: &str = r##"{
  "type": "FeatureCollection",
  "features": [
    {"type":"Feature","properties":{"population":100},
     "geometry":{"type":"Polygon","coordinates":[
       [[0,0],[10,0],[10,10],[0,10],[0,0]],
       [[3,3],[3,7],[7,7],[7,3],[3,3]]]}},
    {"type":"Feature","properties":{"population":400},
     "geometry":{"type":"MultiPolygon","coordinates":[
       [[[12,0],[16,0],[16,4],[12,4],[12,0]]],
       [[[12,6],[16,6],[16,10],[12,10],[12,6]]]]}}
  ]
}"##;

#[test]
fn test_choropleth_polygons_render_with_evenodd_fill() {
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Scale(fill: population, gradient: ["#f7fbff", "#08306b"])
            Space(geom, projection: "equirectangular") {
                Geo(fill: population, stroke: "#ffffff", strokeWidth: 0.5)
            }
        }"##,
        POLYGONS,
    )
    .svg;

    // Two areal features -> two <path> elements with even-odd fill.
    assert_eq!(svg.matches("<path").count(), 2);
    assert_eq!(svg.matches("fill-rule=\"evenodd\"").count(), 2);
    assert!(svg.contains("algraf-geom-geo"));
    // The polygon-with-hole produces two rings (two `M` subpaths) in one path.
    let first_path = svg.split("<path").nth(1).unwrap();
    assert_eq!(
        first_path[..first_path.find("/>").unwrap()]
            .matches('M')
            .count(),
        2
    );
    // Gradient endpoints: low population light, high population dark.
    assert!(svg.contains("#f7fbff"));
    assert!(svg.contains("#08306b"));
    // A spatial space draws no axes or grid.
    assert!(!svg.contains("algraf-axis"));
    assert!(!svg.contains("algraf-grid"));
}

#[test]
fn test_albers_usa_alias_resolves_via_proj4rs() {
    // A US-ish coordinate projects without error through the Albers alias.
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Space(geom, projection: "albers_usa") { Geo(fill: "#cccccc") }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},
           "geometry":{"type":"Polygon","coordinates":[
             [[-100,30],[-95,30],[-95,35],[-100,35],[-100,30]]]}}]}"##,
    )
    .svg;
    assert_eq!(svg.matches("<path").count(), 1);
}

#[test]
fn test_albers_usa_composite_renders_all_three_regions() {
    // A lower-48 polygon, an Alaska polygon, and a Hawaii point all route
    // through the `albers_usa` composite and render (spec §16.14).
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 600, height: 400) {
            Theme(name: "void")
            Space(geom, projection: "albers_usa") { Geo(fill: "#cccccc", stroke: "#333") }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[
             [[-100,30],[-95,30],[-95,35],[-100,35],[-100,30]]]}},
          {"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[
             [[-152,62],[-148,62],[-148,66],[-152,66],[-152,62]]]}},
          {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[-157,20]}}]}"##,
    )
    .svg;
    // Two polygons render as paths; the Hawaii point renders as a circle.
    assert_eq!(svg.matches("<path").count(), 2);
    assert_eq!(svg.matches("<circle").count(), 1);
}

#[test]
fn test_centroid_stat_renders_points() {
    // `Centroid(geom)` turns each polygon into a point, so the derived `Geo`
    // layer draws one circle per feature (spec §15.13).
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Derive centers = Centroid(geom)
            Space(geom, data: centers, projection: "equirectangular") {
                Geo(fill: "#cc3333")
            }
        }"##,
        POLYGONS,
    )
    .svg;
    assert_eq!(svg.matches("<circle").count(), 2);
}

#[test]
fn test_simplify_stat_renders_polygons() {
    // `Simplify(geom)` keeps polygons areal, so the derived layer still paths.
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Derive simple = Simplify(geom, tolerance: 0.5)
            Space(geom, data: simple, projection: "equirectangular") {
                Geo(fill: "#cccccc", stroke: "#333")
            }
        }"##,
        POLYGONS,
    )
    .svg;
    assert_eq!(svg.matches("<path").count(), 2);
}

#[test]
fn test_spatial_join_tags_points_with_polygon_attribute() {
    // Two points; the first falls inside region A's square, the second inside
    // region B's square. The join appends the region `name`, and a `Geo` over the
    // joined table colors each point by its region (spec §15.14).
    let svg = render_geojson_with_table(
        r##"Chart(data: GeoJson("pts.geojson")) {
            Theme(name: "void")
            Table regions = GeoJson("regions.geojson")
            Derive tagged = SpatialJoin(geom, table: regions, predicate: "within")
            Space(geom, data: tagged, projection: "equirectangular") {
                Geo(fill: name)
            }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[1,1]}},
          {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[11,1]}}]}"##,
        "regions",
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{"name":"A"},
           "geometry":{"type":"Polygon","coordinates":[[[0,0],[5,0],[5,5],[0,5],[0,0]]]}},
          {"type":"Feature","properties":{"name":"B"},
           "geometry":{"type":"Polygon","coordinates":[[[10,0],[15,0],[15,5],[10,5],[10,0]]]}}]}"##,
    )
    .svg;
    // Two joined points render as two circles, fill-colored by the matched region.
    assert_eq!(svg.matches("<circle").count(), 2);
}

#[test]
fn test_antimeridian_crossing_line_breaks_instead_of_charting_across() {
    // A line from 170°E to 170°W (a 20° span the short way, but a 340° jump the
    // wrong way) must break into two subpaths rather than draw one long chord
    // across the map (spec §16.15).
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Space(geom, projection: "equirectangular") { Geo(stroke: "#000", strokeWidth: 1) }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},
           "geometry":{"type":"LineString","coordinates":[[170,10],[-170,10]]}}]}"##,
    )
    .svg;
    // The single antimeridian-crossing segment yields two `M` move commands
    // (two subpaths) in the path data instead of one move + one line.
    let path = svg.split("<path").nth(1).unwrap();
    let d = &path[..path.find("/>").unwrap()];
    assert_eq!(d.matches('M').count(), 2, "expected a broken line: {d}");
}

#[test]
fn test_long_segment_is_resampled_into_intermediate_points() {
    // A polygon with 30°-long edges is densified, so the projected ring has more
    // vertices than the four source corners (spec §16.15). Under Mercator the
    // resampled edges follow the projection.
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Space(geom, projection: "mercator") { Geo(fill: "#ccc") }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},
           "geometry":{"type":"Polygon","coordinates":[
             [[-30,-30],[30,-30],[30,30],[-30,30],[-30,-30]]]}}]}"##,
    )
    .svg;
    let path = svg.split("<path").nth(1).unwrap();
    let d = &path[..path.find("/>").unwrap()];
    // Four source edges, each >5°, resample to several `L` commands apiece.
    assert!(
        d.matches('L').count() > 4,
        "expected resampled vertices, got {} in {d}",
        d.matches('L').count()
    );
}

#[test]
fn test_invalid_projection_reports_e1802() {
    let result = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Space(geom, projection: "not_a_projection") { Geo(fill: "#ccc") }
        }"##,
        POLYGONS,
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E1802"),
        "expected E1802, got {:?}",
        result.diagnostics
    );
}

#[test]
fn test_graticule_renders_projected_grid_lines() {
    // The graticule draws meridian/parallel lines through the active spatial
    // scale across several projections (spec §14.24).
    for projection in ["equirectangular", "mercator", "albers"] {
        let svg = render_geojson(
            &format!(
                r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {{
                    Theme(name: "void")
                    Space(geom, projection: "{projection}") {{
                        Geo(fill: "#cccccc")
                        Graticule(stroke: "#999999", strokeWidth: 0.5, step: 5)
                    }}
                }}"##
            ),
            POLYGONS,
        )
        .svg;
        assert!(
            svg.contains("algraf-geom-graticule"),
            "{projection}: missing graticule layer"
        );
        // The graticule path is stroked, unfilled, and uses the requested color.
        assert!(
            svg.contains("#999999"),
            "{projection}: missing stroke color"
        );
        assert!(
            svg.matches("<path").count() >= 2,
            "{projection}: expected a graticule path alongside the geometry"
        );
    }
}

#[test]
fn test_graticule_in_lonlat_projected_space() {
    // A projected `long * lat` overlay space is spatial, so a graticule renders.
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Space(geom, projection: "equirectangular") {
                Geo(fill: "#cccccc")
                Graticule()
            }
        }"##,
        POLYGONS,
    )
    .svg;
    assert!(svg.contains("algraf-geom-graticule"));
}

#[test]
fn test_point_feature_renders_as_circle() {
    let svg = render_geojson(
        r##"Chart(data: GeoJson("x.geojson"), width: 400, height: 300) {
            Theme(name: "void")
            Space(geom) { Geo(fill: "#cc3333") }
        }"##,
        r##"{"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[5,5]}},
          {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[8,2]}}]}"##,
    )
    .svg;
    assert_eq!(svg.matches("<circle").count(), 2);
}
