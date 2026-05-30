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
#[cfg(feature = "sql")]
pub mod sqlite;
/// When built without the `sql` feature (e.g. the WASM runtime), SQLite loading
/// is replaced by stubs that report the capability as unavailable rather than
/// linking the native `libsqlite3-sys` C library (spec §30, §10.12).
#[cfg(not(feature = "sql"))]
#[path = "sqlite_stub.rs"]
pub mod sqlite;
pub mod temporal;
pub mod topojson;
pub mod value;

/// Re-export the geometry vocabulary so downstream crates share one
/// `geo_types` version without each depending on it directly (spec §10.11).
pub use geo_types;

pub use csv::{
    read_csv, read_csv_path, read_csv_schema, read_csv_schema_str, read_csv_str,
    read_csv_str_with_temporal_policy, read_csv_with_temporal_policy, read_delimited,
    read_delimited_schema, read_delimited_schema_with_temporal_policy,
    read_delimited_with_temporal_policy, read_tsv, read_tsv_str, read_tsv_with_temporal_policy,
    LoadResult, DEFAULT_SCHEMA_SAMPLE,
};
pub use error::{DataError, DataWarning};
pub use format::{
    read_bytes, read_bytes_as, read_bytes_as_with_temporal_policy, read_format,
    read_format_with_temporal_policy, read_path, read_path_as, read_schema_bytes,
    read_schema_bytes_as, read_schema_bytes_as_with_temporal_policy, read_schema_format,
    read_schema_format_with_temporal_policy, read_schema_path, read_schema_path_as, Format,
};
pub use frame::{Column, DataFrame, RowView, Table};
pub use geojson::{read_geojson, read_geojson_str, GEOMETRY_COLUMN};
pub use json::{read_json, read_json_str, read_ndjson, read_ndjson_str};
pub use schema::{ColumnDef, DataType};
pub use shapefile::{read_shapefile_bundle, read_shapefile_path, ShapefileBundle};
pub use sqlite::{read_sqlite_path, read_sqlite_schema_path};
pub use temporal::{
    parse_anchor_date, parse_temporal, parse_temporal_literal, validate_temporal_format, EpochUnit,
    ParseErrorPolicy, ParsedTemporal, TemporalColumnParse, TemporalParsePolicy, TemporalParseType,
    TemporalTimezone,
};
pub use topojson::{read_topojson, read_topojson_str};
pub use value::{DataValue, DataValueRef, DateTimeValue, TemporalPrecision};
