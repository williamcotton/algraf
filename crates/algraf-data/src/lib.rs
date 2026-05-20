//! CSV loading, schema inference, dataframe, and type inference.
//!
//! See spec §10 (data sources). The [`Table`] trait is the access boundary;
//! the concrete [`DataFrame`] storage is internal to the runtime and must not
//! leak into parser, semantics, LSP, or renderer interfaces (spec §10.5).

pub mod csv;
pub mod error;
pub mod frame;
pub mod infer;
pub mod schema;
pub mod temporal;
pub mod value;

pub use csv::{
    read_csv, read_csv_path, read_csv_schema, read_csv_schema_str, read_csv_str, LoadResult,
    DEFAULT_SCHEMA_SAMPLE,
};
pub use error::{DataError, DataWarning};
pub use frame::{Column, DataFrame, RowView, Table};
pub use schema::{ColumnDef, DataType};
pub use temporal::{parse_temporal, ParsedTemporal};
pub use value::{DataValue, DataValueRef, DateTimeValue, TemporalPrecision};
