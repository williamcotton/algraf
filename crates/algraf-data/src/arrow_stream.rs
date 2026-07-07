//! Arrow IPC stream loading for caller-provided data (spec §10.14).

use std::io::Read;

use crate::error::DataError;
use crate::schema::ColumnDef;
use crate::LoadResult;

#[cfg(feature = "arrow-stream")]
use std::io::Cursor;

#[cfg(feature = "arrow-stream")]
use arrow_array::RecordBatchReader;
#[cfg(feature = "arrow-stream")]
use arrow_ipc::reader::StreamReader;
#[cfg(feature = "arrow-stream")]
use arrow_schema::ArrowError;

#[cfg(feature = "arrow-stream")]
use crate::arrow_convert::{
    read_record_batches, schema_defs, ArrowConversionContext, UnsupportedTypePolicy,
};

#[cfg(feature = "arrow-stream")]
const CONTEXT: ArrowConversionContext = ArrowConversionContext::ArrowStream;
#[cfg(feature = "arrow-stream")]
const POLICY: UnsupportedTypePolicy = UnsupportedTypePolicy::Error;

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
    schema_defs(reader.schema(), POLICY, CONTEXT)
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
    read_record_batches(reader, POLICY, CONTEXT, arrow_stream_error)
}

#[cfg(feature = "arrow-stream")]
fn arrow_stream_error(err: ArrowError) -> DataError {
    DataError::ArrowStream(err.to_string())
}
