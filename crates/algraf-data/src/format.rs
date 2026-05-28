//! Data source format selection and dispatch (spec §10.1, §10.2, §10.11).
//!
//! A source's format is chosen by its file extension, or named explicitly by a
//! source constructor (`GeoJson`/`Shapefile`). Loading any format yields the
//! same [`DataFrame`](crate::DataFrame) shape, so downstream parser, semantics,
//! and render code stay format-agnostic (spec §10.5).

use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use crate::csv::{
    read_csv_with_temporal_policy, read_delimited_schema_with_temporal_policy,
    read_tsv_with_temporal_policy, LoadResult,
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
            "shapefile" | "shp" => Ok(Format::Shapefile),
            _ => Err(format!(
                "unsupported data format {value:?}; expected csv, tsv, json, ndjson, geojson, or topojson"
            )),
        }
    }
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
    reader: R,
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
    reader: R,
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
    }
}
