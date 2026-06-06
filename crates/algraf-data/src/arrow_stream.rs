//! Arrow IPC stream loading for caller-provided data (spec §10.14).

use std::io::Read;

use crate::error::DataError;
use crate::schema::ColumnDef;
use crate::LoadResult;

#[cfg(feature = "arrow-stream")]
use std::io::Cursor;
#[cfg(feature = "arrow-stream")]
use std::sync::Arc;

#[cfg(feature = "arrow-stream")]
use arrow_array::{
    Array, BooleanArray, Date32Array, Date64Array, Float32Array, Float64Array, Int16Array,
    Int32Array, Int64Array, Int8Array, LargeStringArray, RecordBatch, RecordBatchReader,
    StringArray, TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
#[cfg(feature = "arrow-stream")]
use arrow_ipc::reader::StreamReader;
#[cfg(feature = "arrow-stream")]
use arrow_schema::{ArrowError, DataType as ArrowDataType, Field, SchemaRef, TimeUnit};
#[cfg(feature = "arrow-stream")]
use chrono::{DateTime, Duration, NaiveDate};

#[cfg(feature = "arrow-stream")]
use crate::frame::{Column, DataFrame};
#[cfg(feature = "arrow-stream")]
use crate::schema::DataType;
#[cfg(feature = "arrow-stream")]
use crate::value::{DateTimeValue, TemporalPrecision};

#[cfg(feature = "arrow-stream")]
pub fn read_arrow_stream<R: Read>(reader: R) -> Result<LoadResult, DataError> {
    let reader = StreamReader::try_new(reader, None).map_err(arrow_stream_error)?;
    read_from_reader(reader)
}

#[cfg(not(feature = "arrow-stream"))]
pub fn read_arrow_stream<R: Read>(_reader: R) -> Result<LoadResult, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
pub fn read_arrow_stream_bytes(bytes: &[u8]) -> Result<LoadResult, DataError> {
    read_arrow_stream(Cursor::new(bytes))
}

#[cfg(not(feature = "arrow-stream"))]
pub fn read_arrow_stream_bytes(_bytes: &[u8]) -> Result<LoadResult, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
pub fn read_arrow_stream_schema<R: Read>(reader: R) -> Result<Vec<ColumnDef>, DataError> {
    let reader = StreamReader::try_new(reader, None).map_err(arrow_stream_error)?;
    schema_defs(reader.schema())
}

#[cfg(not(feature = "arrow-stream"))]
pub fn read_arrow_stream_schema<R: Read>(_reader: R) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
pub fn read_arrow_stream_schema_bytes(bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    read_arrow_stream_schema(Cursor::new(bytes))
}

#[cfg(not(feature = "arrow-stream"))]
pub fn read_arrow_stream_schema_bytes(_bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
fn read_from_reader<R>(reader: R) -> Result<LoadResult, DataError>
where
    R: RecordBatchReader,
{
    let schema = reader.schema();
    let mut builders = column_builders(&schema)?;
    for batch in reader {
        append_batch(&batch.map_err(arrow_stream_error)?, &mut builders)?;
    }
    let schema = builders.iter().map(ColumnBuilder::def).collect();
    let columns = builders.into_iter().map(ColumnBuilder::finish).collect();
    Ok(LoadResult {
        frame: DataFrame::new(schema, columns),
        warnings: Vec::new(),
    })
}

#[cfg(feature = "arrow-stream")]
fn schema_defs(schema: SchemaRef) -> Result<Vec<ColumnDef>, DataError> {
    schema
        .fields()
        .iter()
        .map(|field| {
            Ok(ColumnDef {
                name: field.name().clone(),
                dtype: arrow_dtype(field.data_type())?,
                nullable: field.is_nullable(),
                examples: vec![],
            })
        })
        .collect()
}

#[cfg(feature = "arrow-stream")]
fn column_builders(schema: &SchemaRef) -> Result<Vec<ColumnBuilder>, DataError> {
    schema.fields().iter().map(ColumnBuilder::new).collect()
}

#[cfg(feature = "arrow-stream")]
fn append_batch(batch: &RecordBatch, builders: &mut [ColumnBuilder]) -> Result<(), DataError> {
    for (idx, builder) in builders.iter_mut().enumerate() {
        let array = batch.column(idx);
        builder.append_array(array.as_ref())?;
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn arrow_dtype(dtype: &ArrowDataType) -> Result<DataType, DataError> {
    match dtype {
        ArrowDataType::Boolean => Ok(DataType::Boolean),
        ArrowDataType::Int8
        | ArrowDataType::Int16
        | ArrowDataType::Int32
        | ArrowDataType::Int64
        | ArrowDataType::UInt8
        | ArrowDataType::UInt16
        | ArrowDataType::UInt32
        | ArrowDataType::UInt64 => Ok(DataType::Integer),
        ArrowDataType::Float32 | ArrowDataType::Float64 => Ok(DataType::Float),
        ArrowDataType::Date32 | ArrowDataType::Date64 | ArrowDataType::Timestamp(_, _) => {
            Ok(DataType::Temporal)
        }
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => Ok(DataType::String),
        other => Err(unsupported_type(other)),
    }
}

#[cfg(feature = "arrow-stream")]
enum ColumnBuilder {
    Bool {
        name: String,
        nullable: bool,
        examples: Vec<String>,
        values: Vec<Option<bool>>,
    },
    Int {
        name: String,
        nullable: bool,
        examples: Vec<String>,
        values: Vec<Option<i64>>,
    },
    Float {
        name: String,
        nullable: bool,
        examples: Vec<String>,
        values: Vec<Option<f64>>,
    },
    Temporal {
        name: String,
        nullable: bool,
        examples: Vec<String>,
        values: Vec<Option<DateTimeValue>>,
    },
    String {
        name: String,
        nullable: bool,
        examples: Vec<String>,
        values: Vec<Option<String>>,
    },
}

#[cfg(feature = "arrow-stream")]
impl ColumnBuilder {
    fn new(field: &Arc<Field>) -> Result<Self, DataError> {
        let name = field.name().clone();
        let nullable = field.is_nullable();
        Ok(match arrow_dtype(field.data_type())? {
            DataType::Boolean => ColumnBuilder::Bool {
                name,
                nullable,
                examples: Vec::new(),
                values: Vec::new(),
            },
            DataType::Integer => ColumnBuilder::Int {
                name,
                nullable,
                examples: Vec::new(),
                values: Vec::new(),
            },
            DataType::Float => ColumnBuilder::Float {
                name,
                nullable,
                examples: Vec::new(),
                values: Vec::new(),
            },
            DataType::Temporal => ColumnBuilder::Temporal {
                name,
                nullable,
                examples: Vec::new(),
                values: Vec::new(),
            },
            DataType::String | DataType::Mixed | DataType::Unknown | DataType::Geometry => {
                ColumnBuilder::String {
                    name,
                    nullable,
                    examples: Vec::new(),
                    values: Vec::new(),
                }
            }
        })
    }

    fn def(&self) -> ColumnDef {
        let (name, dtype, nullable, examples) = match self {
            ColumnBuilder::Bool {
                name,
                nullable,
                examples,
                ..
            } => (name, DataType::Boolean, nullable, examples),
            ColumnBuilder::Int {
                name,
                nullable,
                examples,
                ..
            } => (name, DataType::Integer, nullable, examples),
            ColumnBuilder::Float {
                name,
                nullable,
                examples,
                ..
            } => (name, DataType::Float, nullable, examples),
            ColumnBuilder::Temporal {
                name,
                nullable,
                examples,
                ..
            } => (name, DataType::Temporal, nullable, examples),
            ColumnBuilder::String {
                name,
                nullable,
                examples,
                ..
            } => (name, DataType::String, nullable, examples),
        };
        ColumnDef {
            name: name.clone(),
            dtype,
            nullable: *nullable,
            examples: examples.clone(),
        }
    }

    fn append_array(&mut self, array: &dyn Array) -> Result<(), DataError> {
        self.reserve(array.len());
        match self {
            ColumnBuilder::Bool {
                examples, values, ..
            } => append_bool(array, examples, values),
            ColumnBuilder::Int {
                examples, values, ..
            } => append_int(array, examples, values),
            ColumnBuilder::Float {
                examples, values, ..
            } => append_float(array, examples, values),
            ColumnBuilder::Temporal {
                examples, values, ..
            } => append_temporal(array, examples, values),
            ColumnBuilder::String {
                examples, values, ..
            } => append_string(array, examples, values),
        }
    }

    fn reserve(&mut self, additional: usize) {
        match self {
            ColumnBuilder::Bool { values, .. } => values.reserve(additional),
            ColumnBuilder::Int { values, .. } => values.reserve(additional),
            ColumnBuilder::Float { values, .. } => values.reserve(additional),
            ColumnBuilder::Temporal { values, .. } => values.reserve(additional),
            ColumnBuilder::String { values, .. } => values.reserve(additional),
        }
    }

    fn finish(self) -> Column {
        match self {
            ColumnBuilder::Bool { values, .. } => Column::from_bool_options(values),
            ColumnBuilder::Int { values, .. } => Column::from_int_options(values),
            ColumnBuilder::Float { values, .. } => Column::from_float_options(values),
            ColumnBuilder::Temporal { values, .. } => Column::from_temporal_options(values),
            ColumnBuilder::String { values, .. } => Column::String(values),
        }
    }
}

#[cfg(feature = "arrow-stream")]
fn append_bool(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<bool>>,
) -> Result<(), DataError> {
    let array = downcast::<BooleanArray>(array)?;
    for idx in 0..array.len() {
        let value = (!array.is_null(idx)).then(|| array.value(idx));
        push_example(examples, value.map(|v| v.to_string()));
        out.push(value);
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn append_int(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<i64>>,
) -> Result<(), DataError> {
    macro_rules! append {
        ($ty:ty, $cast:expr) => {{
            let array = downcast::<$ty>(array)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| $cast(array.value(idx)));
                push_example(examples, value.map(|v| v.to_string()));
                out.push(value);
            }
            return Ok(());
        }};
    }
    match array.data_type() {
        ArrowDataType::Int8 => append!(Int8Array, i64::from),
        ArrowDataType::Int16 => append!(Int16Array, i64::from),
        ArrowDataType::Int32 => append!(Int32Array, i64::from),
        ArrowDataType::Int64 => append!(Int64Array, |v| v),
        ArrowDataType::UInt8 => append!(UInt8Array, i64::from),
        ArrowDataType::UInt16 => append!(UInt16Array, i64::from),
        ArrowDataType::UInt32 => append!(UInt32Array, i64::from),
        ArrowDataType::UInt64 => append!(UInt64Array, |v| i64::try_from(v).unwrap_or(i64::MAX)),
        other => Err(unsupported_type(other)),
    }
}

#[cfg(feature = "arrow-stream")]
fn append_float(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<f64>>,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Float32 => {
            let array = downcast::<Float32Array>(array)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| f64::from(array.value(idx)));
                push_example(examples, value.map(|v| v.to_string()));
                out.push(value);
            }
        }
        ArrowDataType::Float64 => {
            let array = downcast::<Float64Array>(array)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx));
                push_example(examples, value.map(|v| v.to_string()));
                out.push(value);
            }
        }
        other => return Err(unsupported_type(other)),
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn append_temporal(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<DateTimeValue>>,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Date32 => {
            let array = downcast::<Date32Array>(array)?;
            for idx in 0..array.len() {
                let value = if array.is_null(idx) {
                    None
                } else {
                    date32_value(array.value(idx))
                };
                push_example(examples, value.map(|v| v.instant.date().to_string()));
                out.push(value);
            }
        }
        ArrowDataType::Date64 => {
            let array = downcast::<Date64Array>(array)?;
            for idx in 0..array.len() {
                let value = if array.is_null(idx) {
                    None
                } else {
                    timestamp_value(array.value(idx), TimeUnit::Millisecond)
                };
                push_example(examples, value.map(|v| v.instant.to_string()));
                out.push(value);
            }
        }
        ArrowDataType::Timestamp(unit, _) => append_timestamp(array, *unit, examples, out)?,
        other => return Err(unsupported_type(other)),
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn append_timestamp(
    array: &dyn Array,
    unit: TimeUnit,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<DateTimeValue>>,
) -> Result<(), DataError> {
    macro_rules! append {
        ($ty:ty) => {{
            let array = downcast::<$ty>(array)?;
            for idx in 0..array.len() {
                let value = if array.is_null(idx) {
                    None
                } else {
                    timestamp_value(array.value(idx), unit)
                };
                push_example(examples, value.map(|v| v.instant.to_string()));
                out.push(value);
            }
        }};
    }
    match unit {
        TimeUnit::Second => append!(TimestampSecondArray),
        TimeUnit::Millisecond => append!(TimestampMillisecondArray),
        TimeUnit::Microsecond => append!(TimestampMicrosecondArray),
        TimeUnit::Nanosecond => append!(TimestampNanosecondArray),
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn append_string(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<String>>,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Utf8 => {
            let array = downcast::<StringArray>(array)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx).to_string());
                push_example(examples, value.clone());
                out.push(value);
            }
        }
        ArrowDataType::LargeUtf8 => {
            let array = downcast::<LargeStringArray>(array)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx).to_string());
                push_example(examples, value.clone());
                out.push(value);
            }
        }
        other => return Err(unsupported_type(other)),
    }
    Ok(())
}

#[cfg(feature = "arrow-stream")]
fn date32_value(days: i32) -> Option<DateTimeValue> {
    let date =
        NaiveDate::from_ymd_opt(1970, 1, 1)?.checked_add_signed(Duration::days(days.into()))?;
    Some(DateTimeValue::new(
        date.and_hms_opt(0, 0, 0)?,
        TemporalPrecision::Date,
    ))
}

#[cfg(feature = "arrow-stream")]
fn timestamp_value(value: i64, unit: TimeUnit) -> Option<DateTimeValue> {
    let micros = match unit {
        TimeUnit::Second => value.checked_mul(1_000_000)?,
        TimeUnit::Millisecond => value.checked_mul(1_000)?,
        TimeUnit::Microsecond => value,
        TimeUnit::Nanosecond => value / 1_000,
    };
    DateTime::from_timestamp_micros(micros)
        .map(|dt| DateTimeValue::new(dt.naive_utc(), TemporalPrecision::DateTime))
}

#[cfg(feature = "arrow-stream")]
fn push_example(examples: &mut Vec<String>, value: Option<String>) {
    if examples.len() < 3 {
        if let Some(value) = value {
            examples.push(value);
        }
    }
}

#[cfg(feature = "arrow-stream")]
fn downcast<T: Array + 'static>(array: &dyn Array) -> Result<&T, DataError> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| unsupported_type(array.data_type()))
}

#[cfg(feature = "arrow-stream")]
fn unsupported_type(dtype: &ArrowDataType) -> DataError {
    DataError::ArrowStream(format!("unsupported Arrow stream column type {dtype:?}"))
}

#[cfg(feature = "arrow-stream")]
fn arrow_stream_error(err: ArrowError) -> DataError {
    DataError::ArrowStream(err.to_string())
}
