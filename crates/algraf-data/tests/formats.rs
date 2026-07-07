//! TSV, JSON, and NDJSON loading and inference tests (spec §10.2, §10.3, §27.1).

use algraf_data::{
    read_bytes, read_json_str, read_ndjson_str, read_schema_bytes, read_tsv_str, DataError,
    DataType, DataValueRef, Format, Table,
};
use std::path::Path;

fn dtype(frame: &algraf_data::DataFrame, column: &str) -> DataType {
    frame.column_def(column).expect("column exists").dtype
}

// --- Format selection by extension (spec §10.2) -----------------------------

#[test]
fn test_format_from_extension() {
    assert_eq!(Format::from_path(Path::new("a.csv")), Format::Csv);
    assert_eq!(Format::from_path(Path::new("a.tsv")), Format::Tsv);
    assert_eq!(Format::from_path(Path::new("a.tab")), Format::Tsv);
    assert_eq!(Format::from_path(Path::new("a.json")), Format::Json);
    assert_eq!(Format::from_path(Path::new("a.ndjson")), Format::NdJson);
    assert_eq!(Format::from_path(Path::new("a.jsonl")), Format::NdJson);
    assert_eq!(Format::from_path(Path::new("a.parquet")), Format::Parquet);
    // Case-insensitive.
    assert_eq!(Format::from_path(Path::new("A.JSON")), Format::Json);
    // Unknown and missing extensions fall back to CSV.
    assert_eq!(Format::from_path(Path::new("a.arrow")), Format::Csv);
    assert_eq!(Format::from_path(Path::new("a.txt")), Format::Csv);
    assert_eq!(Format::from_path(Path::new("a")), Format::Csv);
}

#[test]
fn test_byte_readers_dispatch_by_extension() {
    let frame = read_bytes(Path::new("data.tsv"), b"x\ty\n1\t2\n")
        .expect("tsv bytes load")
        .frame;
    assert_eq!(dtype(&frame, "x"), DataType::Integer);

    let schema = read_schema_bytes(Path::new("rows.ndjson"), b"{\"label\":\"a\"}\n", 10)
        .expect("schema bytes load");
    assert_eq!(schema[0].name, "label");
    assert_eq!(schema[0].dtype, DataType::String);
}

#[cfg(not(feature = "arrow-stream"))]
#[test]
fn arrow_stream_disabled_feature_stubs_report_capability() {
    let err = algraf_data::read_bytes_as(b"", Format::ArrowStream).unwrap_err();
    assert!(err.to_string().contains("not enabled"));

    let err = algraf_data::read_schema_bytes_as(b"", Format::ArrowStream, 10).unwrap_err();
    assert!(err.to_string().contains("not enabled"));
}

#[cfg(not(feature = "parquet"))]
#[test]
fn parquet_disabled_feature_stubs_report_capability() {
    let err = algraf_data::read_bytes_as(b"", Format::Parquet).unwrap_err();
    assert!(err.to_string().contains("not enabled"));

    let err = algraf_data::read_schema_bytes_as(b"", Format::Parquet, 10).unwrap_err();
    assert!(err.to_string().contains("not enabled"));
}

// --- TSV (spec §10.2) -------------------------------------------------------

#[test]
fn test_tsv_headers_and_columns() {
    let frame = read_tsv_str("a\tb\tc\n1\t2\t3\n").expect("tsv loads").frame;
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    assert_eq!(frame.row_count(), 1);
}

#[test]
fn test_tsv_inference_matches_csv() {
    let frame = read_tsv_str("n\tlabel\n1\tx\n2\ty\n")
        .expect("tsv loads")
        .frame;
    assert_eq!(dtype(&frame, "n"), DataType::Integer);
    assert_eq!(dtype(&frame, "label"), DataType::String);
}

#[test]
fn test_tsv_duplicate_headers_rejected() {
    let err = read_tsv_str("a\ta\n1\t2\n").unwrap_err();
    assert!(matches!(err, DataError::DuplicateHeader(_)));
}

// --- JSON (spec §10.2, §10.3) -----------------------------------------------

#[test]
fn test_json_array_of_objects() {
    let frame = read_json_str(r#"[{"region":"west","revenue":10},{"region":"east","revenue":20}]"#)
        .expect("json loads")
        .frame;
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names, vec!["region", "revenue"]);
    assert_eq!(frame.row_count(), 2);
    assert_eq!(dtype(&frame, "region"), DataType::String);
    assert_eq!(dtype(&frame, "revenue"), DataType::Integer);
}

#[test]
fn test_json_preserves_key_order() {
    // Column order follows first-seen key order, not alphabetical.
    let frame = read_json_str(r#"[{"z":1,"a":2}]"#)
        .expect("json loads")
        .frame;
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names, vec!["z", "a"]);
}

#[test]
fn test_json_native_types_infer_like_csv() {
    let frame = read_json_str(r#"[{"f":1.5,"b":true},{"f":2.0,"b":false}]"#)
        .expect("json loads")
        .frame;
    assert_eq!(dtype(&frame, "f"), DataType::Float);
    assert_eq!(dtype(&frame, "b"), DataType::Boolean);
    assert_eq!(frame.value("b", 0), Some(DataValueRef::Bool(true)));
}

#[test]
fn test_json_null_is_missing() {
    let frame = read_json_str(r#"[{"x":1},{"x":null},{"x":3}]"#)
        .expect("json loads")
        .frame;
    assert_eq!(dtype(&frame, "x"), DataType::Integer);
    assert_eq!(frame.value("x", 1), Some(DataValueRef::Null));
}

#[test]
fn test_json_missing_key_backfills() {
    // A key absent from earlier rows is a missing cell; a new key appears as a
    // column the moment it is first seen, with prior rows backfilled.
    let frame = read_json_str(r#"[{"a":1},{"a":2,"b":9}]"#)
        .expect("json loads")
        .frame;
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names, vec!["a", "b"]);
    assert_eq!(frame.value("b", 0), Some(DataValueRef::Null));
    assert_eq!(frame.value("b", 1), Some(DataValueRef::Int(9)));
}

#[test]
fn test_json_top_level_must_be_array() {
    let err = read_json_str(r#"{"a":1}"#).unwrap_err();
    assert!(matches!(err, DataError::JsonNotArray));
}

#[test]
fn test_json_row_must_be_object() {
    let err = read_json_str(r#"[{"a":1}, 5]"#).unwrap_err();
    assert!(matches!(err, DataError::JsonRowNotObject { index: 1 }));
}

#[test]
fn test_json_malformed_input() {
    let err = read_json_str("{not json").unwrap_err();
    assert!(matches!(err, DataError::Json(_)));
}

// --- NDJSON (spec §10.2) ----------------------------------------------------

#[test]
fn test_ndjson_one_object_per_line() {
    let frame = read_ndjson_str("{\"x\":1,\"y\":\"a\"}\n{\"x\":2,\"y\":\"b\"}\n")
        .expect("ndjson loads")
        .frame;
    assert_eq!(frame.row_count(), 2);
    assert_eq!(dtype(&frame, "x"), DataType::Integer);
    assert_eq!(dtype(&frame, "y"), DataType::String);
}

#[test]
fn test_ndjson_skips_blank_lines() {
    let frame = read_ndjson_str("{\"x\":1}\n\n{\"x\":2}\n")
        .expect("ndjson loads")
        .frame;
    assert_eq!(frame.row_count(), 2);
}

#[test]
fn test_ndjson_reports_line_on_parse_error() {
    let err = read_ndjson_str("{\"x\":1}\n{bad}\n").unwrap_err();
    assert!(matches!(err, DataError::NdJson { line: 2, .. }));
}

#[test]
fn test_ndjson_line_must_be_object() {
    let err = read_ndjson_str("{\"x\":1}\n42\n").unwrap_err();
    assert!(matches!(err, DataError::NdJsonRowNotObject { line: 2 }));
}
