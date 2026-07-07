#![cfg(feature = "arrow-stream")]

use std::sync::Arc;

use algraf_data::{
    read_bytes_as, read_schema_bytes_as, sniff_caller_input_format, DataType, DataValueRef, Format,
    SniffedFormat, Table, TemporalPrecision,
};
use arrow_array::{
    ArrayRef, BooleanArray, Date32Array, Date64Array, Float32Array, Float64Array, Int64Array,
    RecordBatch, StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray, UInt64Array,
};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType as ArrowDataType, Field, Schema, TimeUnit};

fn arrow_stream_fixture() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", ArrowDataType::Int64, false),
        Field::new("label", ArrowDataType::Utf8, true),
        Field::new("flag", ArrowDataType::Boolean, true),
        Field::new("score", ArrowDataType::Float64, true),
        Field::new("day", ArrowDataType::Date32, true),
        Field::new(
            "ts",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
            Arc::new(StringArray::from(vec![Some("north"), None])) as ArrayRef,
            Arc::new(BooleanArray::from(vec![Some(true), Some(false)])) as ArrayRef,
            Arc::new(Float64Array::from(vec![Some(2.5), None])) as ArrayRef,
            Arc::new(Date32Array::from(vec![Some(0), None])) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(vec![Some(1_000_000), None])) as ArrayRef,
        ],
    )
    .unwrap();

    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
    writer.write(&batch).unwrap();
    writer.finish().unwrap();
    drop(writer);
    bytes
}

fn arrow_stream_with_schema(schema: Arc<Schema>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
    writer.finish().unwrap();
    drop(writer);
    bytes
}

#[test]
fn loads_arrow_stream_scalars_and_nulls() {
    let bytes = arrow_stream_fixture();
    let frame = read_bytes_as(&bytes, Format::ArrowStream)
        .expect("arrow stream loads")
        .frame;

    assert_eq!(frame.row_count(), 2);
    assert_eq!(frame.column_def("id").unwrap().dtype, DataType::Integer);
    assert_eq!(frame.column_def("label").unwrap().dtype, DataType::String);
    assert_eq!(frame.column_def("flag").unwrap().dtype, DataType::Boolean);
    assert_eq!(frame.column_def("score").unwrap().dtype, DataType::Float);
    assert_eq!(frame.column_def("day").unwrap().dtype, DataType::Temporal);
    assert_eq!(frame.column_def("ts").unwrap().dtype, DataType::Temporal);
    assert!(frame.column_def("score").unwrap().nullable);

    assert_eq!(frame.value("id", 0), Some(DataValueRef::Int(1)));
    assert_eq!(frame.value("label", 0), Some(DataValueRef::String("north")));
    assert_eq!(frame.value("label", 1), Some(DataValueRef::Null));
    assert_eq!(frame.value("flag", 1), Some(DataValueRef::Bool(false)));
    assert_eq!(frame.value("score", 1), Some(DataValueRef::Null));
    match frame.value("day", 0).expect("day value") {
        DataValueRef::Temporal(value) => assert_eq!(value.precision, TemporalPrecision::Date),
        other => panic!("expected temporal date, got {other:?}"),
    }
    match frame.value("ts", 0).expect("timestamp value") {
        DataValueRef::Temporal(value) => {
            assert_eq!(value.precision, TemporalPrecision::DateTime);
            assert_eq!(value.instant.and_utc().timestamp(), 1);
        }
        other => panic!("expected temporal timestamp, got {other:?}"),
    }
}

#[test]
fn loads_arrow_stream_numeric_and_temporal_edge_cases() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("wide", ArrowDataType::UInt64, true),
        Field::new("f32", ArrowDataType::Float32, true),
        Field::new("f64", ArrowDataType::Float64, true),
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
            Arc::new(Date64Array::from(vec![Some(86_400_000)])) as ArrayRef,
            Arc::new(TimestampSecondArray::from(vec![Some(1)])) as ArrayRef,
            Arc::new(TimestampMillisecondArray::from(vec![Some(2_000)])) as ArrayRef,
            Arc::new(TimestampNanosecondArray::from(vec![Some(3_000_000_000)])) as ArrayRef,
        ],
    )
    .unwrap();

    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
    writer.write(&batch).unwrap();
    writer.finish().unwrap();
    drop(writer);

    let frame = read_bytes_as(&bytes, Format::ArrowStream)
        .expect("arrow stream loads")
        .frame;

    assert_eq!(frame.value("wide", 0), Some(DataValueRef::Int(i64::MAX)));
    assert_eq!(frame.value("f32", 0), Some(DataValueRef::Float(1.25)));
    assert_eq!(frame.value("f64", 0), Some(DataValueRef::Float(2.5)));
    match frame.value("date64", 0).expect("date64 value") {
        DataValueRef::Temporal(value) => {
            assert_eq!(value.precision, TemporalPrecision::DateTime);
            assert_eq!(value.instant.and_utc().timestamp(), 86_400);
        }
        other => panic!("expected temporal date64 value, got {other:?}"),
    }
    for (column, second) in [("ts_s", 1), ("ts_ms", 2), ("ts_ns", 3)] {
        match frame.value(column, 0).expect("timestamp value") {
            DataValueRef::Temporal(value) => {
                assert_eq!(value.precision, TemporalPrecision::DateTime);
                assert_eq!(value.instant.and_utc().timestamp(), second);
            }
            other => panic!("expected temporal timestamp for {column}, got {other:?}"),
        }
    }
}

#[test]
fn arrow_stream_schema_uses_arrow_metadata() {
    let schema = read_schema_bytes_as(&arrow_stream_fixture(), Format::ArrowStream, 10)
        .expect("arrow schema loads");

    assert_eq!(schema[0].name, "id");
    assert_eq!(schema[0].dtype, DataType::Integer);
    assert_eq!(schema[3].name, "score");
    assert!(schema[3].nullable);
    assert!(schema[3].examples.is_empty());
}

#[test]
fn arrow_stream_rejects_unsupported_types_by_policy() {
    let item = Arc::new(Field::new("item", ArrowDataType::Int64, true));
    let nested = vec![Field::new("nested", ArrowDataType::Utf8, true)].into();
    let unsupported = vec![
        ArrowDataType::Float16,
        ArrowDataType::Binary,
        ArrowDataType::Decimal128(10, 2),
        ArrowDataType::List(item),
        ArrowDataType::Struct(nested),
        ArrowDataType::Dictionary(
            Box::new(ArrowDataType::Int32),
            Box::new(ArrowDataType::Utf8),
        ),
    ];

    for dtype in unsupported {
        let schema = Arc::new(Schema::new(vec![Field::new("bad", dtype.clone(), true)]));
        let err = read_schema_bytes_as(&arrow_stream_with_schema(schema), Format::ArrowStream, 10)
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("unsupported Arrow stream column type"),
            "unexpected error for {dtype:?}: {message}"
        );
    }
}

#[test]
fn sniffing_selects_arrow_stream_and_csv_fallback() {
    let bytes = arrow_stream_fixture();
    assert_eq!(
        sniff_caller_input_format(&bytes),
        SniffedFormat::Supported(Format::ArrowStream)
    );
    assert_eq!(
        sniff_caller_input_format(b"x,y\n1,2\n"),
        SniffedFormat::Supported(Format::Csv)
    );
    assert_eq!(
        sniff_caller_input_format(b"ARROW1\0\0"),
        SniffedFormat::Unsupported("Arrow IPC file")
    );
}

#[test]
fn parses_arrow_stream_format_names() {
    assert_eq!(
        "arrow-stream".parse::<Format>().unwrap(),
        Format::ArrowStream
    );
    assert_eq!("arrow".parse::<Format>().unwrap(), Format::ArrowStream);
}
