//! Geometry column storage, GeoJSON, and shapefile loading (spec §10.11, §27.1).

use std::path::PathBuf;

use algraf_data::geo_types::{Geometry, LineString, Point, Polygon};
use algraf_data::{
    read_geojson_str, read_path, read_shapefile_bundle, read_shapefile_path, Column, ColumnDef,
    DataFrame, DataType, DataValueRef, ShapefileBundle, Table,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn dtype(frame: &DataFrame, column: &str) -> DataType {
    frame.column_def(column).expect("column exists").dtype
}

/// A geometry column round-trips through the dataframe and reports its type.
#[test]
fn test_geometry_column_round_trip() {
    let pt: Geometry<f64> = Point::new(5.0, 5.0).into();
    let line: Geometry<f64> = LineString::from(vec![(0.0, 0.0), (1.0, 2.0), (3.0, 0.0)]).into();

    let schema = vec![ColumnDef {
        name: "geom".to_string(),
        dtype: DataType::Geometry,
        nullable: true,
        examples: Vec::new(),
    }];
    let columns = vec![Column::Geometry(vec![
        Some(pt.clone()),
        None,
        Some(line.clone()),
    ])];
    let frame = DataFrame::new(schema, columns);

    assert_eq!(frame.row_count(), 3);
    assert_eq!(frame.column_def("geom").unwrap().dtype, DataType::Geometry);

    match frame.value("geom", 0) {
        Some(DataValueRef::Geometry(g)) => assert_eq!(g, &pt),
        other => panic!("expected geometry, got {other:?}"),
    }
    // The present-but-missing cell borrows as Null.
    assert!(frame.value("geom", 1).unwrap().is_null());
    match frame.value("geom", 2) {
        Some(DataValueRef::Geometry(g)) => assert_eq!(g, &line),
        other => panic!("expected geometry, got {other:?}"),
    }
}

/// Geometry is its own kind: never continuous, never categorical (spec §10.11).
#[test]
fn test_geometry_type_is_neither_continuous_nor_categorical() {
    assert!(!DataType::Geometry.is_continuous());
    assert!(!DataType::Geometry.is_categorical());
    assert!(DataType::Geometry.is_geometry());
}

/// A polygon-with-hole survives storage and borrow unchanged.
#[test]
fn test_polygon_with_hole_round_trip() {
    let exterior = LineString::from(vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ]);
    let hole = LineString::from(vec![
        (3.0, 3.0),
        (3.0, 7.0),
        (7.0, 7.0),
        (7.0, 3.0),
        (3.0, 3.0),
    ]);
    let poly: Geometry<f64> = Polygon::new(exterior, vec![hole]).into();

    let frame = DataFrame::new(
        vec![ColumnDef {
            name: "geom".to_string(),
            dtype: DataType::Geometry,
            nullable: false,
            examples: Vec::new(),
        }],
        vec![Column::Geometry(vec![Some(poly.clone())])],
    );

    match frame.value("geom", 0) {
        Some(DataValueRef::Geometry(Geometry::Polygon(p))) => {
            assert_eq!(p.interiors().len(), 1);
            assert_eq!(&Geometry::Polygon(p.clone()), &poly);
        }
        other => panic!("expected polygon, got {other:?}"),
    }
}

// --- GeoJSON loading (spec §10.11) -------------------------------------------

/// A FeatureCollection loads one row per feature with properties typed by the
/// shared inference pipeline and geometry in the `geom` column.
#[test]
fn test_geojson_feature_collection_loads() {
    let frame = read_geojson_str(
        r#"{
          "type": "FeatureCollection",
          "features": [
            {"type":"Feature","properties":{"name":"a","pop":100},
             "geometry":{"type":"Point","coordinates":[1,2]}},
            {"type":"Feature","properties":{"name":"b","pop":250},
             "geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]}}
          ]
        }"#,
    )
    .expect("geojson loads")
    .frame;

    assert_eq!(frame.row_count(), 2);
    // Properties type-infer exactly as CSV/JSON would.
    assert_eq!(dtype(&frame, "name"), DataType::String);
    assert_eq!(dtype(&frame, "pop"), DataType::Integer);
    assert_eq!(dtype(&frame, "geom"), DataType::Geometry);
    // The geometry column is appended last.
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names.last(), Some(&"geom"));

    assert!(matches!(
        frame.value("geom", 0),
        Some(DataValueRef::Geometry(Geometry::Point(_)))
    ));
    assert!(matches!(
        frame.value("geom", 1),
        Some(DataValueRef::Geometry(Geometry::LineString(_)))
    ));
}

/// A bare GeoJSON geometry (no FeatureCollection/Feature) is a parse error.
#[test]
fn test_geojson_bare_geometry_rejected() {
    let err = read_geojson_str(r#"{"type":"Point","coordinates":[0,0]}"#).unwrap_err();
    assert!(matches!(err, algraf_data::DataError::Geo(_)));
}

/// The checked-in fixture exercises the full geometry dispatch surface.
#[test]
fn test_geojson_fixture_full_dispatch() {
    let frame = read_path(&fixture("tiny.geojson")).expect("loads").frame;
    assert_eq!(frame.row_count(), 4);
    assert_eq!(dtype(&frame, "name"), DataType::String);
    assert_eq!(dtype(&frame, "population"), DataType::Integer);
    assert_eq!(dtype(&frame, "density"), DataType::Float);
    assert_eq!(dtype(&frame, "geom"), DataType::Geometry);

    let kinds: Vec<&str> = (0..4)
        .map(|r| match frame.value("geom", r) {
            Some(DataValueRef::Geometry(g)) => match g {
                Geometry::Point(_) => "Point",
                Geometry::LineString(_) => "LineString",
                Geometry::Polygon(_) => "Polygon",
                Geometry::MultiPolygon(_) => "MultiPolygon",
                _ => "other",
            },
            _ => "null",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["Point", "LineString", "Polygon", "MultiPolygon"]
    );
}

// --- Shapefile loading (spec §10.11) -----------------------------------------

/// The shapefile fixture decodes to the same DataFrame shape as GeoJSON:
/// attribute columns through inference plus a `geom` geometry column.
#[test]
fn test_shapefile_fixture_loads() {
    let frame = read_shapefile_path(&fixture("tiny.shp"))
        .expect("loads")
        .frame;
    assert_eq!(frame.row_count(), 2);
    assert_eq!(dtype(&frame, "name"), DataType::String);
    assert_eq!(dtype(&frame, "population"), DataType::Integer);
    assert_eq!(dtype(&frame, "density"), DataType::Float);
    assert_eq!(dtype(&frame, "geom"), DataType::Geometry);

    // The shapefile polygon type can hold multiple parts, so the reader
    // normalizes every areal feature to a MultiPolygon — including the
    // single polygon-with-hole. The `Geo` mark renders both uniformly.
    let (poly_with_hole, multi) = (frame.value("geom", 0), frame.value("geom", 1));
    assert!(matches!(
        poly_with_hole,
        Some(DataValueRef::Geometry(Geometry::MultiPolygon(_)))
    ));
    assert!(matches!(
        multi,
        Some(DataValueRef::Geometry(Geometry::MultiPolygon(_)))
    ));
    // The polygon-with-hole's single part retains its interior ring.
    if let Some(DataValueRef::Geometry(Geometry::MultiPolygon(mp))) = poly_with_hole {
        assert_eq!(mp.0.len(), 1);
        assert_eq!(mp.0[0].interiors().len(), 1);
    }
}

/// A `.shp` path also loads via the extension-dispatching `read_path`.
#[test]
fn test_shapefile_via_read_path() {
    let frame = read_path(&fixture("tiny.shp")).expect("loads").frame;
    assert_eq!(frame.row_count(), 2);
    assert_eq!(dtype(&frame, "geom"), DataType::Geometry);
}

/// The sidecar-bundle reader matches path-backed shapefile loading for the
/// checked-in fixture.
#[test]
fn test_shapefile_bundle_reader_matches_path_reader() {
    let path_frame = read_shapefile_path(&fixture("tiny.shp"))
        .expect("path loads")
        .frame;
    let shp = std::fs::read(fixture("tiny.shp")).unwrap();
    let dbf = std::fs::read(fixture("tiny.dbf")).unwrap();
    let shx = std::fs::read(fixture("tiny.shx")).unwrap();

    let bundle_frame = read_shapefile_bundle(ShapefileBundle {
        shp: &shp,
        dbf: &dbf,
        shx: Some(&shx),
    })
    .expect("bundle loads")
    .frame;

    assert_eq!(bundle_frame.row_count(), path_frame.row_count());
    let mut bundle_names: Vec<&str> = bundle_frame.column_names().collect();
    let mut path_names: Vec<&str> = path_frame.column_names().collect();
    bundle_names.sort_unstable();
    path_names.sort_unstable();
    assert_eq!(bundle_names, path_names);
    assert_eq!(dtype(&bundle_frame, "geom"), DataType::Geometry);
}
