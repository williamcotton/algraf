//! Native Parquet loading for the CLI path.

use std::fs::File;
use std::path::Path;

use bytes::Bytes;
use parquet::arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ProjectionMask};

use crate::arrow_convert::{
    read_record_batches, schema_defs, ArrowConversionContext, UnsupportedTypePolicy,
};
use crate::error::DataError;
use crate::schema::ColumnDef;
use crate::LoadResult;

const CONTEXT: ArrowConversionContext = ArrowConversionContext::Parquet;
const POLICY: UnsupportedTypePolicy = UnsupportedTypePolicy::FallbackToString;

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
    schema_defs(builder.schema().clone(), POLICY, CONTEXT)
}

pub fn read_parquet_schema_bytes(bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))
        .map_err(parquet_error)?;
    schema_defs(builder.schema().clone(), POLICY, CONTEXT)
}

fn read_from_builder<R>(
    builder: ParquetRecordBatchReaderBuilder<R>,
) -> Result<LoadResult, DataError>
where
    R: parquet::file::reader::ChunkReader + 'static,
{
    let reader = builder.build().map_err(parquet_error)?;
    read_record_batches(reader, POLICY, CONTEXT, arrow_error)
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

fn parquet_error(err: parquet::errors::ParquetError) -> DataError {
    DataError::Parquet(err.to_string())
}

fn arrow_error(err: arrow_schema::ArrowError) -> DataError {
    DataError::Parquet(err.to_string())
}
