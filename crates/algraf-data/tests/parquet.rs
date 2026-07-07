#![cfg(feature = "parquet")]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use algraf_data::{
    read_parquet_bytes, read_parquet_path, read_parquet_path_projected, read_parquet_schema_path,
    ColumnView, DataError, DataType, DataValueRef, Table, TemporalPrecision,
};
use arrow_array::{
    types::{ArrowPrimitiveType, Float16Type},
    ArrayRef, BooleanArray, Date32Array, Date64Array, Float16Array, Float32Array, Float64Array,
    Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray, UInt64Array,
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

fn write_schema_only_fixture(path: &Path, fields: Vec<Field>) {
    let schema = Arc::new(Schema::new(fields));
    let file = File::create(path).unwrap();
    let writer = ArrowWriter::try_new(file, schema, None).unwrap();
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
fn reads_parquet_numeric_and_temporal_edge_cases() {
    let path = temp_file("numeric-temporal");
    let schema = Arc::new(Schema::new(vec![
        Field::new("wide", ArrowDataType::UInt64, true),
        Field::new("f32", ArrowDataType::Float32, true),
        Field::new("f64", ArrowDataType::Float64, true),
        Field::new("date32", ArrowDataType::Date32, true),
        Field::new("date64", ArrowDataType::Date64, true),
        Field::new(
            "ts_s",
            ArrowDataType::Timestamp(TimeUnit::Second, None),
            true,
        ),
        Field::new(
            "ts_ms",
            ArrowDataType::Timestamp(TimeUnit::Millisecond, None),
            true,
        ),
        Field::new(
            "ts_us",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            "ts_ns",
            ArrowDataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        ),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(UInt64Array::from(vec![Some((i64::MAX as u64) + 1)])) as ArrayRef,
            Arc::new(Float32Array::from(vec![Some(1.25)])) as ArrayRef,
            Arc::new(Float64Array::from(vec![Some(2.5)])) as ArrayRef,
            Arc::new(Date32Array::from(vec![Some(1)])) as ArrayRef,
            Arc::new(Date64Array::from(vec![Some(86_400_000)])) as ArrayRef,
            Arc::new(TimestampSecondArray::from(vec![Some(1)])) as ArrayRef,
            Arc::new(TimestampMillisecondArray::from(vec![Some(2_000)])) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(vec![Some(3_000_000)])) as ArrayRef,
            Arc::new(TimestampNanosecondArray::from(vec![Some(4_000_000_000)])) as ArrayRef,
        ],
    )
    .unwrap();
    let file = File::create(&path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let frame = read_parquet_path(&path).unwrap().frame;

    assert_eq!(frame.value("wide", 0), Some(DataValueRef::Int(i64::MAX)));
    assert_eq!(frame.value("f32", 0), Some(DataValueRef::Float(1.25)));
    assert_eq!(frame.value("f64", 0), Some(DataValueRef::Float(2.5)));
    match frame.value("date32", 0).expect("date32 value") {
        DataValueRef::Temporal(value) => {
            assert_eq!(value.precision, TemporalPrecision::Date);
            assert_eq!(value.instant.and_utc().timestamp(), 86_400);
        }
        other => panic!("expected temporal date32 value, got {other:?}"),
    }
    match frame.value("date64", 0).expect("date64 value") {
        DataValueRef::Temporal(value) => {
            assert_eq!(value.precision, TemporalPrecision::DateTime);
            assert_eq!(value.instant.and_utc().timestamp(), 86_400);
        }
        other => panic!("expected temporal date64 value, got {other:?}"),
    }
    for (column, second) in [("ts_s", 1), ("ts_ms", 2), ("ts_us", 3), ("ts_ns", 4)] {
        match frame.value(column, 0).expect("timestamp value") {
            DataValueRef::Temporal(value) => {
                assert_eq!(value.precision, TemporalPrecision::DateTime);
                assert_eq!(value.instant.and_utc().timestamp(), second);
            }
            other => panic!("expected temporal timestamp for {column}, got {other:?}"),
        }
    }

    let _ = fs::remove_file(path);
}

#[test]
fn parquet_schema_falls_back_for_unsupported_types() {
    let path = temp_file("unsupported-schema");
    let item = Arc::new(Field::new("item", ArrowDataType::Int64, true));
    let nested = vec![Field::new("nested", ArrowDataType::Utf8, true)].into();
    write_schema_only_fixture(
        &path,
        vec![
            Field::new("binary", ArrowDataType::Binary, true),
            Field::new("decimal", ArrowDataType::Decimal128(10, 2), true),
            Field::new("list", ArrowDataType::List(item), true),
            Field::new("struct", ArrowDataType::Struct(nested), true),
            Field::new(
                "dictionary",
                ArrowDataType::Dictionary(
                    Box::new(ArrowDataType::Int32),
                    Box::new(ArrowDataType::Utf8),
                ),
                true,
            ),
        ],
    );

    let schema = read_parquet_schema_path(&path).unwrap();

    assert!(schema
        .iter()
        .all(|def| matches!(def.dtype, DataType::String)));

    let _ = fs::remove_file(path);
}

#[test]
fn parquet_preserves_float16_schema_then_value_error() {
    let path = temp_file("float16");
    type F16 = <Float16Type as ArrowPrimitiveType>::Native;
    let schema = Arc::new(Schema::new(vec![Field::new(
        "half",
        ArrowDataType::Float16,
        true,
    )]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Float16Array::from(vec![Some(F16::default())])) as ArrayRef],
    )
    .unwrap();
    let file = File::create(&path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let schema = read_parquet_schema_path(&path).unwrap();
    assert_eq!(schema[0].dtype, DataType::Float);

    let err = read_parquet_path(&path).unwrap_err();
    assert!(matches!(err, DataError::Parquet(_)));
    assert!(err.to_string().contains("Float16"));

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
