//! Data source format selection and dispatch (spec §10.1, §10.2, §10.11).
//!
//! A source's format is chosen by its file extension, or named explicitly by a
//! source constructor (`GeoJson`/`Shapefile`). Loading any format yields the
//! same [`DataFrame`](crate::DataFrame) shape, so downstream parser, semantics,
//! and render code stay format-agnostic (spec §10.5).

#[cfg(feature = "arrow-stream")]
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use crate::csv::{
    read_csv_sample_rows, read_csv_with_temporal_policy, read_delimited_sample_rows,
    read_delimited_schema_with_temporal_policy, read_tsv_with_temporal_policy, LoadResult,
    SampleRows,
};
use crate::error::DataError;
use crate::frame::Table;
use crate::geojson::read_geojson;
use crate::json::{read_json_with_temporal_policy, read_ndjson_with_temporal_policy};
use crate::schema::ColumnDef;
use crate::shapefile::read_shapefile_path;
use crate::temporal::TemporalParsePolicy;
use crate::topojson::read_topojson;

/// A supported data source format (spec §10.2, §10.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Comma-separated values; the default for unrecognized extensions.
    Csv,
    /// Tab-separated values (`.tsv`, `.tab`).
    Tsv,
    /// A JSON array of row objects (`.json`).
    Json,
    /// Newline-delimited JSON: one row object per line (`.ndjson`, `.jsonl`).
    NdJson,
    /// A GeoJSON `FeatureCollection` (`.geojson`), also selected by the
    /// `GeoJson` source constructor (spec §10.11).
    GeoJson,
    /// An ESRI shapefile bundle (`.shp` + `.dbf`/`.shx` sidecars), also selected
    /// by the `Shapefile` source constructor (spec §10.11).
    Shapefile,
    /// A TopoJSON `Topology` (`.topojson`), also selected by the `TopoJson`
    /// source constructor (spec §10.11). The extension form decodes the topology's
    /// sole object; the constructor's `object:` argument names one explicitly.
    TopoJson,
    /// An Apache Parquet columnar table (`.parquet`).
    Parquet,
    /// An Apache Arrow IPC stream. This is selected explicitly for
    /// caller-provided data or by caller-input sniffing; path extension
    /// inference does not select it in v0.57.
    ArrowStream,
}

/// Result of caller-input format sniffing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SniffedFormat {
    Supported(Format),
    Unsupported(&'static str),
}

impl Format {
    /// Stable lowercase spelling for CLI flags and embedded request options.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Json => "json",
            Format::NdJson => "ndjson",
            Format::GeoJson => "geojson",
            Format::Shapefile => "shapefile",
            Format::TopoJson => "topojson",
            Format::Parquet => "parquet",
            Format::ArrowStream => "arrow-stream",
        }
    }

    /// Select a format from a path's extension. Unrecognized (or absent)
    /// extensions fall back to [`Format::Csv`] (spec §10.2).
    pub fn from_path(path: &Path) -> Format {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(Format::from_extension)
            .unwrap_or(Format::Csv)
    }

    /// Map a file extension (without the dot, any case) to a format.
    pub fn from_extension(ext: &str) -> Format {
        match ext.to_ascii_lowercase().as_str() {
            "tsv" | "tab" => Format::Tsv,
            "json" => Format::Json,
            "ndjson" | "jsonl" => Format::NdJson,
            "geojson" => Format::GeoJson,
            "topojson" => Format::TopoJson,
            "shp" => Format::Shapefile,
            "parquet" | "parq" => Format::Parquet,
            _ => Format::Csv,
        }
    }
}

impl FromStr for Format {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "csv" => Ok(Format::Csv),
            "tsv" => Ok(Format::Tsv),
            "json" => Ok(Format::Json),
            "ndjson" => Ok(Format::NdJson),
            "geojson" => Ok(Format::GeoJson),
            "topojson" => Ok(Format::TopoJson),
            "parquet" | "parq" => Ok(Format::Parquet),
            "arrow-stream" | "arrow" => Ok(Format::ArrowStream),
            "shapefile" | "shp" => Ok(Format::Shapefile),
            _ => Err(format!(
                "unsupported data format {value:?}; expected csv, tsv, json, ndjson, geojson, topojson, parquet, or arrow-stream"
            )),
        }
    }
}

/// Sniff caller-provided bytes before falling back to CSV (spec §10.2.1,
/// §10.14). The returned format is only a format selection; the caller still
/// passes the original, complete byte slice to the selected decoder.
pub fn sniff_caller_input_format(bytes: &[u8]) -> SniffedFormat {
    if bytes.starts_with(b"ARROW1") {
        return SniffedFormat::Unsupported("Arrow IPC file");
    }
    if bytes.starts_with(b"PAR1") {
        return SniffedFormat::Supported(Format::Parquet);
    }
    if looks_like_arrow_stream(bytes) {
        return SniffedFormat::Supported(Format::ArrowStream);
    }
    SniffedFormat::Supported(Format::Csv)
}

/// Fully load a data source from a path, selecting the format by extension
/// (spec §10.2, §10.3, §10.11).
pub fn read_path(path: &Path) -> Result<LoadResult, DataError> {
    read_path_as(path, Format::from_path(path))
}

/// Fully load a data source from a path using an explicit format. The format is
/// supplied by the source constructor for geospatial sources, which a bare path
/// extension alone would not distinguish (spec §10.11).
pub fn read_path_as(path: &Path, format: Format) -> Result<LoadResult, DataError> {
    match format {
        // The shapefile reader opens the `.dbf`/`.shx` sidecars itself, so it
        // takes the path rather than an already-opened reader.
        Format::Shapefile => read_shapefile_path(path),
        Format::Parquet => read_parquet_path_dispatch(path),
        _ => {
            let file = std::fs::File::open(path)?;
            read_format(file, format)
        }
    }
}

/// Fully load a data source from bytes, selecting the format by extension.
pub fn read_bytes(path: &Path, bytes: &[u8]) -> Result<LoadResult, DataError> {
    read_bytes_as(bytes, Format::from_path(path))
}

/// Fully load a single-file data source from bytes using an explicit format.
///
/// [`Format::Shapefile`] is a multi-file bundle; use
/// [`crate::read_shapefile_bundle`] for in-memory shapefile sidecars.
pub fn read_bytes_as(bytes: &[u8], format: Format) -> Result<LoadResult, DataError> {
    read_bytes_as_with_temporal_policy(bytes, format, None)
}

pub fn read_bytes_as_with_temporal_policy(
    bytes: &[u8],
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    match format {
        Format::Shapefile => Err(DataError::Geo(
            "a shapefile must be loaded from a sidecar bundle, not a byte slice".to_string(),
        )),
        Format::Parquet => read_parquet_bytes_dispatch(bytes),
        Format::ArrowStream => read_arrow_stream_dispatch(bytes),
        _ => read_format_with_temporal_policy(bytes, format, temporal_policy),
    }
}

/// Fully load a data source from a reader using an explicit format.
///
/// [`Format::Shapefile`] is not loadable from a bare reader (it needs sidecar
/// files resolved by path); use [`read_path_as`] for shapefiles.
pub fn read_format<R: Read>(reader: R, format: Format) -> Result<LoadResult, DataError> {
    read_format_with_temporal_policy(reader, format, None)
}

pub fn read_format_with_temporal_policy<R: Read>(
    mut reader: R,
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    match format {
        Format::Csv => read_csv_with_temporal_policy(reader, temporal_policy),
        Format::Tsv => read_tsv_with_temporal_policy(reader, temporal_policy),
        Format::Json => read_json_with_temporal_policy(reader, temporal_policy),
        Format::NdJson => read_ndjson_with_temporal_policy(reader, temporal_policy),
        Format::GeoJson => read_geojson(reader),
        Format::TopoJson => read_topojson(reader, None),
        Format::Shapefile => Err(DataError::Geo(
            "a shapefile must be loaded from a path, not a stream".to_string(),
        )),
        Format::Parquet => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            read_parquet_bytes_dispatch(&bytes)
        }
        Format::ArrowStream => read_arrow_stream_reader_dispatch(reader),
    }
}

/// Infer a provisional schema from a path, selecting the format by extension.
/// `sample` bounds the rows read for delimited formats; other formats parse the
/// whole document (spec §10.3, §10.11).
pub fn read_schema_path(path: &Path, sample: usize) -> Result<Vec<ColumnDef>, DataError> {
    read_schema_path_as(path, Format::from_path(path), sample)
}

/// Infer a provisional schema from a path using an explicit format.
pub fn read_schema_path_as(
    path: &Path,
    format: Format,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    match format {
        Format::Shapefile => Ok(read_shapefile_path(path)?.frame.schema().to_vec()),
        Format::Parquet => read_parquet_schema_path_dispatch(path),
        _ => {
            let file = std::fs::File::open(path)?;
            read_schema_format(file, format, sample)
        }
    }
}

/// Infer a provisional schema from data source bytes, selecting format by path.
pub fn read_schema_bytes(
    path: &Path,
    bytes: &[u8],
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    read_schema_bytes_as(bytes, Format::from_path(path), sample)
}

/// Infer a provisional schema from single-file data source bytes.
///
/// [`Format::Shapefile`] is a multi-file bundle; use
/// [`crate::read_shapefile_bundle`] and inspect the loaded frame schema.
pub fn read_schema_bytes_as(
    bytes: &[u8],
    format: Format,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    read_schema_bytes_as_with_temporal_policy(bytes, format, sample, None)
}

/// Sample raw source rows for delimited byte-backed formats. Non-delimited
/// formats return `Ok(None)` because compact row rendering for nested source
/// formats is intentionally not part of this helper.
pub fn read_sample_rows_bytes_as(
    bytes: &[u8],
    format: Format,
    sample: usize,
) -> Result<Option<SampleRows>, DataError> {
    match format {
        Format::Csv => read_csv_sample_rows(bytes, sample).map(Some),
        Format::Tsv => read_delimited_sample_rows(bytes, b'\t', sample).map(Some),
        Format::Json
        | Format::NdJson
        | Format::GeoJson
        | Format::Shapefile
        | Format::TopoJson
        | Format::Parquet
        | Format::ArrowStream => Ok(None),
    }
}

pub fn read_schema_bytes_as_with_temporal_policy(
    bytes: &[u8],
    format: Format,
    sample: usize,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DataError> {
    match format {
        Format::Shapefile => Err(DataError::Geo(
            "a shapefile must be loaded from a sidecar bundle, not a byte slice".to_string(),
        )),
        Format::Parquet => read_parquet_schema_bytes_dispatch(bytes),
        Format::ArrowStream => read_arrow_stream_schema_dispatch(bytes),
        _ => read_schema_format_with_temporal_policy(bytes, format, sample, temporal_policy),
    }
}

/// Infer a provisional schema from a reader using an explicit format.
pub fn read_schema_format<R: Read>(
    reader: R,
    format: Format,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    read_schema_format_with_temporal_policy(reader, format, sample, None)
}

pub fn read_schema_format_with_temporal_policy<R: Read>(
    mut reader: R,
    format: Format,
    sample: usize,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DataError> {
    match format {
        Format::Csv => {
            read_delimited_schema_with_temporal_policy(reader, b',', sample, temporal_policy)
        }
        Format::Tsv => {
            read_delimited_schema_with_temporal_policy(reader, b'\t', sample, temporal_policy)
        }
        Format::Json => Ok(read_json_with_temporal_policy(reader, temporal_policy)?
            .frame
            .schema()
            .to_vec()),
        Format::NdJson => Ok(read_ndjson_with_temporal_policy(reader, temporal_policy)?
            .frame
            .schema()
            .to_vec()),
        Format::GeoJson => Ok(read_geojson(reader)?.frame.schema().to_vec()),
        Format::TopoJson => Ok(read_topojson(reader, None)?.frame.schema().to_vec()),
        Format::Shapefile => Err(DataError::Geo(
            "a shapefile must be loaded from a path, not a stream".to_string(),
        )),
        Format::Parquet => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            read_parquet_schema_bytes_dispatch(&bytes)
        }
        Format::ArrowStream => read_arrow_stream_schema_reader_dispatch(reader),
    }
}

fn looks_like_arrow_stream(bytes: &[u8]) -> bool {
    if !has_plausible_arrow_stream_prefix(bytes) {
        return false;
    }
    looks_like_arrow_stream_dispatch(bytes)
}

fn has_plausible_arrow_stream_prefix(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let first = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if first == -1 {
        let len = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        return len > 0 && (len as usize) <= bytes.len().saturating_sub(8);
    }
    first > 0 && (first as usize) <= bytes.len().saturating_sub(4)
}

#[cfg(feature = "arrow-stream")]
fn looks_like_arrow_stream_dispatch(bytes: &[u8]) -> bool {
    arrow_ipc::reader::StreamReader::try_new(Cursor::new(bytes), None).is_ok()
}

#[cfg(not(feature = "arrow-stream"))]
fn looks_like_arrow_stream_dispatch(bytes: &[u8]) -> bool {
    has_plausible_arrow_stream_prefix(bytes)
}

#[cfg(feature = "parquet")]
fn read_parquet_path_dispatch(path: &Path) -> Result<LoadResult, DataError> {
    crate::parquet::read_parquet_path(path)
}

#[cfg(not(feature = "parquet"))]
fn read_parquet_path_dispatch(_path: &Path) -> Result<LoadResult, DataError> {
    Err(DataError::Parquet(
        "Parquet support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "parquet")]
fn read_parquet_bytes_dispatch(bytes: &[u8]) -> Result<LoadResult, DataError> {
    crate::parquet::read_parquet_bytes(bytes)
}

#[cfg(not(feature = "parquet"))]
fn read_parquet_bytes_dispatch(_bytes: &[u8]) -> Result<LoadResult, DataError> {
    Err(DataError::Parquet(
        "Parquet support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "parquet")]
fn read_parquet_schema_path_dispatch(path: &Path) -> Result<Vec<ColumnDef>, DataError> {
    crate::parquet::read_parquet_schema_path(path)
}

#[cfg(not(feature = "parquet"))]
fn read_parquet_schema_path_dispatch(_path: &Path) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::Parquet(
        "Parquet support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "parquet")]
fn read_parquet_schema_bytes_dispatch(bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    crate::parquet::read_parquet_schema_bytes(bytes)
}

#[cfg(not(feature = "parquet"))]
fn read_parquet_schema_bytes_dispatch(_bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::Parquet(
        "Parquet support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
fn read_arrow_stream_dispatch(bytes: &[u8]) -> Result<LoadResult, DataError> {
    crate::arrow_stream::read_arrow_stream_bytes(bytes)
}

#[cfg(not(feature = "arrow-stream"))]
fn read_arrow_stream_dispatch(_bytes: &[u8]) -> Result<LoadResult, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
fn read_arrow_stream_reader_dispatch<R: Read>(reader: R) -> Result<LoadResult, DataError> {
    crate::arrow_stream::read_arrow_stream(reader)
}

#[cfg(not(feature = "arrow-stream"))]
fn read_arrow_stream_reader_dispatch<R: Read>(_reader: R) -> Result<LoadResult, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
fn read_arrow_stream_schema_dispatch(bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    crate::arrow_stream::read_arrow_stream_schema_bytes(bytes)
}

#[cfg(not(feature = "arrow-stream"))]
fn read_arrow_stream_schema_dispatch(_bytes: &[u8]) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}

#[cfg(feature = "arrow-stream")]
fn read_arrow_stream_schema_reader_dispatch<R: Read>(
    reader: R,
) -> Result<Vec<ColumnDef>, DataError> {
    crate::arrow_stream::read_arrow_stream_schema(reader)
}

#[cfg(not(feature = "arrow-stream"))]
fn read_arrow_stream_schema_reader_dispatch<R: Read>(
    _reader: R,
) -> Result<Vec<ColumnDef>, DataError> {
    Err(DataError::ArrowStream(
        "Arrow IPC stream support is not enabled in this build".to_string(),
    ))
}
