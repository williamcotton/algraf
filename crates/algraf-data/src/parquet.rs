//! Native Parquet loading for the CLI path.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{
    Array, BooleanArray, Date32Array, Date64Array, Float32Array, Float64Array, Int16Array,
    Int32Array, Int64Array, Int8Array, LargeStringArray, RecordBatch, RecordBatchReader,
    StringArray, TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow_schema::{DataType as ArrowDataType, Field, SchemaRef, TimeUnit};
use bytes::Bytes;
use chrono::{DateTime, Duration, NaiveDate};
use parquet::arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ProjectionMask};

use crate::error::DataError;
use crate::frame::{Column, DataFrame};
use crate::schema::{ColumnDef, DataType};
use crate::value::{DateTimeValue, TemporalPrecision};
use crate::LoadResult;

pub fn read_parquet_path(path: &Path) -> Result<LoadResult, DataError> {
    read_parquet_path_projected(path, None)
}

pub fn read_parquet_path_projected(
    path: &Path,
    columns: Option<&[&str]>,
) -> Result<LoadResult, DataError> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(parquet_error)?;
    let builder = project_builder(builder, columns)?;
    read_from_builder(builder)
}

pub fn read_parquet_bytes(bytes: &[u8]) -> Result<LoadResult, DataError> {
    read_parquet_bytes_projected(bytes, None)
}

pub fn read_parquet_bytes_projected(
    bytes: &[u8],
    columns: Option<&[&str]>,
) -> Result<LoadResult, DataError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))
        .map_err(parquet_error)?;
    let builder = project_builder(builder, columns)?;
    read_from_builder(builder)
}

pub fn read_parquet_schema_path(path: &Path) -> Result<Vec<ColumnDef>, DataError> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(parquet_error)?;
    Ok(schema_defs(builder.schema().clone()))
}

pub fn read_parquet_schema_bytes(bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))
        .map_err(parquet_error)?;
    Ok(schema_defs(builder.schema().clone()))
}

fn read_from_builder<R>(
    builder: ParquetRecordBatchReaderBuilder<R>,
) -> Result<LoadResult, DataError>
where
    R: parquet::file::reader::ChunkReader + 'static,
{
    let reader = builder.build().map_err(parquet_error)?;
    let schema = reader.schema();
    let mut builders = column_builders(&schema)?;
    for batch in reader {
        append_batch(&batch.map_err(arrow_error)?, &mut builders)?;
    }
    let schema = builders.iter().map(ColumnBuilder::def).collect();
    let columns = builders.into_iter().map(ColumnBuilder::finish).collect();
    Ok(LoadResult {
        frame: DataFrame::new(schema, columns),
        warnings: Vec::new(),
    })
}

fn project_builder<R>(
    builder: ParquetRecordBatchReaderBuilder<R>,
    columns: Option<&[&str]>,
) -> Result<ParquetRecordBatchReaderBuilder<R>, DataError>
where
    R: parquet::file::reader::ChunkReader + 'static,
{
    let Some(columns) = columns else {
        return Ok(builder);
    };
    if columns.is_empty() {
        return Ok(builder);
    }

    let schema = builder.schema();
    let mut indices = Vec::new();
    for requested in columns {
        let Some(index) = schema
            .fields()
            .iter()
            .position(|field| field.name() == requested)
        else {
            return Err(DataError::Parquet(format!(
                "unknown Parquet projection column `{requested}`"
            )));
        };
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    let mask = ProjectionMask::roots(builder.parquet_schema(), indices);
    Ok(builder.with_projection(mask))
}

fn schema_defs(schema: SchemaRef) -> Vec<ColumnDef> {
    schema
        .fields()
        .iter()
        .map(|field| ColumnDef {
            name: field.name().clone(),
            dtype: arrow_dtype(field.data_type()),
            nullable: field.is_nullable(),
            examples: vec![],
        })
        .collect()
}

fn column_builders(schema: &SchemaRef) -> Result<Vec<ColumnBuilder>, DataError> {
    schema.fields().iter().map(ColumnBuilder::new).collect()
}

fn append_batch(batch: &RecordBatch, builders: &mut [ColumnBuilder]) -> Result<(), DataError> {
    for (idx, builder) in builders.iter_mut().enumerate() {
        let array = batch.column(idx);
        builder.append_array(array.as_ref())?;
    }
    Ok(())
}

fn arrow_dtype(dtype: &ArrowDataType) -> DataType {
    match dtype {
        ArrowDataType::Boolean => DataType::Boolean,
        ArrowDataType::Int8
        | ArrowDataType::Int16
        | ArrowDataType::Int32
        | ArrowDataType::Int64
        | ArrowDataType::UInt8
        | ArrowDataType::UInt16
        | ArrowDataType::UInt32
        | ArrowDataType::UInt64 => DataType::Integer,
        ArrowDataType::Float16 | ArrowDataType::Float32 | ArrowDataType::Float64 => DataType::Float,
        ArrowDataType::Date32 | ArrowDataType::Date64 | ArrowDataType::Timestamp(_, _) => {
            DataType::Temporal
        }
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => DataType::String,
        _ => DataType::String,
    }
}

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

impl ColumnBuilder {
    fn new(field: &Arc<Field>) -> Result<Self, DataError> {
        let name = field.name().clone();
        let nullable = field.is_nullable();
        Ok(match arrow_dtype(field.data_type()) {
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

fn date32_value(days: i32) -> Option<DateTimeValue> {
    let date =
        NaiveDate::from_ymd_opt(1970, 1, 1)?.checked_add_signed(Duration::days(days.into()))?;
    Some(DateTimeValue::new(
        date.and_hms_opt(0, 0, 0)?,
        TemporalPrecision::Date,
    ))
}

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

fn push_example(examples: &mut Vec<String>, value: Option<String>) {
    if examples.len() < 3 {
        if let Some(value) = value {
            examples.push(value);
        }
    }
}

fn downcast<T: Array + 'static>(array: &dyn Array) -> Result<&T, DataError> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| unsupported_type(array.data_type()))
}

fn unsupported_type(dtype: &ArrowDataType) -> DataError {
    DataError::Parquet(format!("unsupported Parquet column type {dtype:?}"))
}

fn parquet_error(err: parquet::errors::ParquetError) -> DataError {
    DataError::Parquet(err.to_string())
}

fn arrow_error(err: arrow_schema::ArrowError) -> DataError {
    DataError::Parquet(err.to_string())
}
