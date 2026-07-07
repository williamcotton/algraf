//! Shared Arrow `RecordBatch` to Algraf `DataFrame` conversion.
//!
//! This module is internal to `algraf-data`; concrete Arrow-family types stay
//! behind the crate facade.

use std::sync::Arc;

use arrow_array::{
    Array, BooleanArray, Date32Array, Date64Array, Float32Array, Float64Array, Int16Array,
    Int32Array, Int64Array, Int8Array, LargeStringArray, RecordBatch, RecordBatchReader,
    StringArray, TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow_schema::{ArrowError, DataType as ArrowDataType, Field, SchemaRef, TimeUnit};
use chrono::{DateTime, Duration, NaiveDate};

use crate::error::DataError;
use crate::frame::{Column, DataFrame};
use crate::schema::{ColumnDef, DataType};
use crate::value::{DateTimeValue, TemporalPrecision};
use crate::LoadResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsupportedTypePolicy {
    #[cfg(feature = "arrow-stream")]
    Error,
    #[cfg(feature = "parquet")]
    FallbackToString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrowConversionContext {
    #[cfg(feature = "parquet")]
    Parquet,
    #[cfg(feature = "arrow-stream")]
    ArrowStream,
}

impl ArrowConversionContext {
    fn unsupported_type(self, dtype: &ArrowDataType) -> DataError {
        match self {
            #[cfg(feature = "parquet")]
            ArrowConversionContext::Parquet => {
                DataError::Parquet(format!("unsupported Parquet column type {dtype:?}"))
            }
            #[cfg(feature = "arrow-stream")]
            ArrowConversionContext::ArrowStream => {
                DataError::ArrowStream(format!("unsupported Arrow stream column type {dtype:?}"))
            }
        }
    }
}

pub(crate) fn read_record_batches<R>(
    reader: R,
    policy: UnsupportedTypePolicy,
    context: ArrowConversionContext,
    map_batch_error: impl Fn(ArrowError) -> DataError,
) -> Result<LoadResult, DataError>
where
    R: RecordBatchReader,
{
    let schema = reader.schema();
    let mut builders = column_builders(&schema, policy, context)?;
    for batch in reader {
        append_batch(&batch.map_err(&map_batch_error)?, &mut builders, context)?;
    }
    let schema = builders.iter().map(ColumnBuilder::def).collect();
    let columns = builders.into_iter().map(ColumnBuilder::finish).collect();
    Ok(LoadResult {
        frame: DataFrame::new(schema, columns),
        warnings: Vec::new(),
    })
}

pub(crate) fn schema_defs(
    schema: SchemaRef,
    policy: UnsupportedTypePolicy,
    context: ArrowConversionContext,
) -> Result<Vec<ColumnDef>, DataError> {
    schema
        .fields()
        .iter()
        .map(|field| {
            Ok(ColumnDef {
                name: field.name().clone(),
                dtype: arrow_dtype(field.data_type(), policy, context)?,
                nullable: field.is_nullable(),
                examples: vec![],
            })
        })
        .collect()
}

fn column_builders(
    schema: &SchemaRef,
    policy: UnsupportedTypePolicy,
    context: ArrowConversionContext,
) -> Result<Vec<ColumnBuilder>, DataError> {
    schema
        .fields()
        .iter()
        .map(|field| ColumnBuilder::new(field, policy, context))
        .collect()
}

fn append_batch(
    batch: &RecordBatch,
    builders: &mut [ColumnBuilder],
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    for (idx, builder) in builders.iter_mut().enumerate() {
        let array = batch.column(idx);
        builder.append_array(array.as_ref(), context)?;
    }
    Ok(())
}

fn arrow_dtype(
    dtype: &ArrowDataType,
    policy: UnsupportedTypePolicy,
    context: ArrowConversionContext,
) -> Result<DataType, DataError> {
    #[cfg(not(feature = "arrow-stream"))]
    let _ = context;

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
        ArrowDataType::Float16 => match policy {
            // Parquet historically advertised Float16 columns as Float in
            // schema reads, then failed if values were materialized. Preserve
            // that release behavior during the shared-conversion extraction.
            #[cfg(feature = "parquet")]
            UnsupportedTypePolicy::FallbackToString => Ok(DataType::Float),
            #[cfg(feature = "arrow-stream")]
            UnsupportedTypePolicy::Error => Err(context.unsupported_type(dtype)),
        },
        ArrowDataType::Date32 | ArrowDataType::Date64 | ArrowDataType::Timestamp(_, _) => {
            Ok(DataType::Temporal)
        }
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => Ok(DataType::String),
        other => {
            #[cfg(not(feature = "arrow-stream"))]
            let _ = other;

            match policy {
                #[cfg(feature = "arrow-stream")]
                UnsupportedTypePolicy::Error => Err(context.unsupported_type(other)),
                #[cfg(feature = "parquet")]
                UnsupportedTypePolicy::FallbackToString => Ok(DataType::String),
            }
        }
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
    fn new(
        field: &Arc<Field>,
        policy: UnsupportedTypePolicy,
        context: ArrowConversionContext,
    ) -> Result<Self, DataError> {
        let name = field.name().clone();
        let nullable = field.is_nullable();
        Ok(match arrow_dtype(field.data_type(), policy, context)? {
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

    fn append_array(
        &mut self,
        array: &dyn Array,
        context: ArrowConversionContext,
    ) -> Result<(), DataError> {
        self.reserve(array.len());
        match self {
            ColumnBuilder::Bool {
                examples, values, ..
            } => append_bool(array, examples, values, context),
            ColumnBuilder::Int {
                examples, values, ..
            } => append_int(array, examples, values, context),
            ColumnBuilder::Float {
                examples, values, ..
            } => append_float(array, examples, values, context),
            ColumnBuilder::Temporal {
                examples, values, ..
            } => append_temporal(array, examples, values, context),
            ColumnBuilder::String {
                examples, values, ..
            } => append_string(array, examples, values, context),
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

fn append_bool(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<bool>>,
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    let array = downcast::<BooleanArray>(array, context)?;
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
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    macro_rules! append {
        ($ty:ty, $cast:expr) => {{
            let array = downcast::<$ty>(array, context)?;
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
        other => Err(context.unsupported_type(other)),
    }
}

fn append_float(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<f64>>,
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Float32 => {
            let array = downcast::<Float32Array>(array, context)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| f64::from(array.value(idx)));
                push_example(examples, value.map(|v| v.to_string()));
                out.push(value);
            }
        }
        ArrowDataType::Float64 => {
            let array = downcast::<Float64Array>(array, context)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx));
                push_example(examples, value.map(|v| v.to_string()));
                out.push(value);
            }
        }
        other => return Err(context.unsupported_type(other)),
    }
    Ok(())
}

fn append_temporal(
    array: &dyn Array,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<DateTimeValue>>,
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Date32 => {
            let array = downcast::<Date32Array>(array, context)?;
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
            let array = downcast::<Date64Array>(array, context)?;
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
        ArrowDataType::Timestamp(unit, _) => {
            append_timestamp(array, *unit, examples, out, context)?
        }
        other => return Err(context.unsupported_type(other)),
    }
    Ok(())
}

fn append_timestamp(
    array: &dyn Array,
    unit: TimeUnit,
    examples: &mut Vec<String>,
    out: &mut Vec<Option<DateTimeValue>>,
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    macro_rules! append {
        ($ty:ty) => {{
            let array = downcast::<$ty>(array, context)?;
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
    context: ArrowConversionContext,
) -> Result<(), DataError> {
    match array.data_type() {
        ArrowDataType::Utf8 => {
            let array = downcast::<StringArray>(array, context)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx).to_string());
                push_example(examples, value.clone());
                out.push(value);
            }
        }
        ArrowDataType::LargeUtf8 => {
            let array = downcast::<LargeStringArray>(array, context)?;
            for idx in 0..array.len() {
                let value = (!array.is_null(idx)).then(|| array.value(idx).to_string());
                push_example(examples, value.clone());
                out.push(value);
            }
        }
        other => return Err(context.unsupported_type(other)),
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

fn downcast<T: Array + 'static>(
    array: &dyn Array,
    context: ArrowConversionContext,
) -> Result<&T, DataError> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| context.unsupported_type(array.data_type()))
}
