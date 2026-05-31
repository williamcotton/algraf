#![cfg(feature = "parquet")]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use algraf_data::{
    read_parquet_bytes, read_parquet_path, read_parquet_path_projected, read_parquet_schema_path,
    ColumnView, DataType, DataValueRef, Table,
};
use arrow_array::{
    ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::{DataType as ArrowDataType, Field, Schema, TimeUnit};
use parquet::arrow::ArrowWriter;

fn temp_file(test: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "algraf-parquet-{test}-{}-{nanos}.parquet",
        std::process::id()
    ))
}

fn write_fixture(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", ArrowDataType::Int64, false),
        Field::new("group", ArrowDataType::Utf8, true),
        Field::new("metric", ArrowDataType::Float64, true),
        Field::new("flag", ArrowDataType::Boolean, true),
        Field::new(
            "event_time",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("unused_wide", ArrowDataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3, 4])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                Some("alpha"),
                Some("beta"),
                None,
                Some("alpha"),
            ])) as ArrayRef,
            Arc::new(Float64Array::from(vec![
                Some(1.5),
                None,
                Some(3.25),
                Some(4.75),
            ])) as ArrayRef,
            Arc::new(BooleanArray::from(vec![
                Some(true),
                None,
                Some(false),
                Some(true),
            ])) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(vec![
                Some(1_704_067_200_000_000),
                None,
                Some(1_704_153_600_000_000),
                Some(1_704_240_000_000_000),
            ])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                Some("skip-a"),
                Some("skip-b"),
                Some("skip-c"),
                Some("skip-d"),
            ])) as ArrayRef,
        ],
    )
    .unwrap();

    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

#[test]
fn reads_parquet_schema_from_metadata() {
    let path = temp_file("schema");
    write_fixture(&path);

    let schema = read_parquet_schema_path(&path).unwrap();

    assert_eq!(
        schema
            .iter()
            .map(|def| (def.name.as_str(), def.dtype, def.nullable))
            .collect::<Vec<_>>(),
        vec![
            ("id", DataType::Integer, false),
            ("group", DataType::String, true),
            ("metric", DataType::Float, true),
            ("flag", DataType::Boolean, true),
            ("event_time", DataType::Temporal, true),
            ("unused_wide", DataType::String, true),
        ]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn reads_parquet_values_and_preserves_null_semantics() {
    let path = temp_file("values");
    write_fixture(&path);

    let loaded = read_parquet_path(&path).unwrap();
    let frame = loaded.frame;

    assert_eq!(frame.row_count(), 4);
    assert_eq!(frame.value("id", 0), Some(DataValueRef::Int(1)));
    assert_eq!(frame.value("group", 2), Some(DataValueRef::Null));
    assert_eq!(frame.value("metric", 1), Some(DataValueRef::Null));
    assert_eq!(frame.value("flag", 2), Some(DataValueRef::Bool(false)));
    assert_eq!(frame.value("event_time", 1), Some(DataValueRef::Null));
    assert_eq!(frame.value("missing", 0), None);
    assert_eq!(frame.value("metric", 99), None);

    let Some(ColumnView::Float(metric)) = Table::column(&frame, "metric") else {
        panic!("metric should be a float column");
    };
    assert_eq!(metric.len(), 4);
    assert!(!metric.validity().is_valid(1));
    assert_eq!(
        metric.iter_options().collect::<Vec<_>>(),
        vec![Some(1.5), None, Some(3.25), Some(4.75),]
    );

    let bytes = fs::read(&path).unwrap();
    let from_bytes = read_parquet_bytes(&bytes).unwrap();
    assert_eq!(from_bytes.frame.row_count(), 4);

    let _ = fs::remove_file(path);
}

#[test]
fn projected_parquet_load_skips_unreferenced_columns() {
    let path = temp_file("projection");
    write_fixture(&path);

    let loaded = read_parquet_path_projected(&path, Some(&["metric", "group"])).unwrap();
    let frame = loaded.frame;

    assert_eq!(frame.row_count(), 4);
    assert!(frame.schema().iter().any(|def| def.name == "metric"));
    assert!(frame.schema().iter().any(|def| def.name == "group"));
    assert!(frame.schema().iter().all(|def| def.name != "unused_wide"));
    assert_eq!(frame.value("metric", 0), Some(DataValueRef::Float(1.5)));
    assert_eq!(frame.value("unused_wide", 0), None);

    let err = read_parquet_path_projected(&path, Some(&["does_not_exist"])).unwrap_err();
    assert!(err.to_string().contains("does_not_exist"));

    let _ = fs::remove_file(path);
}
