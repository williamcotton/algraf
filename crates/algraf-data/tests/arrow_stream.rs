#![cfg(feature = "arrow-stream")]

use std::sync::Arc;

use algraf_data::{
    read_bytes_as, read_schema_bytes_as, sniff_caller_input_format, DataType, DataValueRef, Format,
    SniffedFormat, Table, TemporalPrecision,
};
use arrow_array::{
    ArrayRef, BooleanArray, Date32Array, Float64Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
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
