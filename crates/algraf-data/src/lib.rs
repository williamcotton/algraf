//! CSV loading, schema inference, dataframe, and type inference.
//!
//! See spec §10 (data sources). The [`Table`] trait is the access boundary;
//! the concrete [`DataFrame`] storage is internal to the runtime and must not
//! leak into parser, semantics, LSP, or renderer interfaces (spec §10.5).

pub mod csv;
pub mod error;
pub mod format;
pub mod frame;
pub mod geojson;
pub mod infer;
pub mod json;
pub mod schema;
pub mod shapefile;
pub mod temporal;
pub mod value;

/// Re-export the geometry vocabulary so downstream crates share one
/// `geo_types` version without each depending on it directly (spec §10.11).
pub use geo_types;

pub use csv::{
    read_csv, read_csv_path, read_csv_schema, read_csv_schema_str, read_csv_str, read_delimited,
    read_delimited_schema, read_tsv, read_tsv_str, LoadResult, DEFAULT_SCHEMA_SAMPLE,
};
pub use error::{DataError, DataWarning};
pub use format::{
    read_format, read_path, read_path_as, read_schema_format, read_schema_path,
    read_schema_path_as, Format,
};
pub use frame::{Column, DataFrame, RowView, Table};
pub use geojson::{read_geojson, read_geojson_str, GEOMETRY_COLUMN};
pub use json::{read_json, read_json_str, read_ndjson, read_ndjson_str};
pub use schema::{ColumnDef, DataType};
pub use shapefile::read_shapefile_path;
pub use temporal::{parse_temporal, ParsedTemporal};
pub use value::{DataValue, DataValueRef, DateTimeValue, TemporalPrecision};
