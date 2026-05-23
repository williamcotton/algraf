//! Spatial rendering: projection, spatial scale, and the `Geo` mark
//! (spec §14.x, §16.14, §16.15, §27.1).

use algraf_data::{read_geojson_str, Table};
use algraf_render::{render, RenderResult, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;

/// Parse + analyze + render `source` against an inline GeoJSON FeatureCollection.
fn render_geojson(source: &str, geojson: &str) -> RenderResult {
    let frame = read_geojson_str(geojson).expect("geojson").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    render(&ir, &frame, &Theme::void(), None).expect("render")
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
