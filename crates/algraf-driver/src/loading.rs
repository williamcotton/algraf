use std::path::{Path, PathBuf};

use algraf_data::{
    read_bytes_as, read_csv, read_csv_schema, read_schema_bytes_as, ColumnDef, DataError,
    DataFrame, DataWarning, Format, LoadResult, Table,
};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::SourceExpr;

use crate::error::{DriverError, LoadContext};
use crate::io::{DriverIo, OsDriverIo};
use crate::resolution::{DataLocation, DriverEnv, ResolvedTableSource, SourceInput};

/// One loaded chart-scoped named table.
#[derive(Debug, Clone)]
pub struct NamedTable {
    pub name: String,
    pub path: PathBuf,
    pub frame: DataFrame,
    pub warnings: Vec<DataWarning>,
}

/// One loaded chart-scoped named table schema.
#[derive(Debug, Clone)]
pub struct NamedTableSchema {
    pub name: String,
    pub path: PathBuf,
    pub schema: Vec<ColumnDef>,
}

/// Load a full data source for a chart.
pub fn load_data(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
) -> Result<LoadResult, DriverError> {
    load_data_with_io(source_expr, source, base_dir, data_override, &OsDriverIo)
}

/// Load a full data source for a chart through an injected I/O provider.
pub fn load_data_with_io(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, false);
    let location = env.resolver().data_location(source_expr)?;
    load_location(location, LoadContext::Primary, io)
}

pub(crate) fn load_primary_with_io(
    location: DataLocation,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    load_location(location, LoadContext::Primary, io)
}

fn load_location(
    location: DataLocation,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    match location {
        DataLocation::Path { path, format } => load_path_with_io(&path, format, context, io),
        DataLocation::Stdin => read_stdin_csv(io),
    }
}

/// Load a full data source from a path.
pub fn load_path(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
) -> Result<LoadResult, DriverError> {
    load_path_with_io(path, format, context, &OsDriverIo)
}

/// Load a full data source from a path through an injected I/O provider.
pub fn load_path_with_io(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io.load_shapefile(path),
        _ => io
            .read_path(path)
            .map_err(DataError::Io)
            .and_then(|bytes| read_bytes_as(bytes.as_slice(), format)),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

/// Load only a data schema, optionally sampling rows for delimited formats.
pub fn load_schema(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    sample_size: Option<usize>,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_schema_with_io(
        source_expr,
        source,
        base_dir,
        data_override,
        sample_size,
        &OsDriverIo,
    )
}

/// Load only a data schema through an injected I/O provider.
pub fn load_schema_with_io(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    sample_size: Option<usize>,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, false);
    let Some(sample_size) = sample_size else {
        return Ok(
            load_data_with_io(source_expr, source, base_dir, data_override, io)?
                .frame
                .schema()
                .to_vec(),
        );
    };

    load_schema_location(env.resolver().data_location(source_expr)?, sample_size, io)
}

fn load_schema_location(
    location: DataLocation,
    sample_size: usize,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    match location {
        DataLocation::Path { path, format } => {
            load_schema_path_with_io(&path, format, sample_size, LoadContext::Primary, io)
        }
        DataLocation::Stdin => read_stdin_csv_schema(sample_size, io),
    }
}

/// Load only a schema from a path.
pub fn load_schema_path(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_schema_path_with_io(path, format, sample_size, context, &OsDriverIo)
}

/// Load only a data schema from a path through an injected I/O provider.
pub fn load_schema_path_with_io(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io
            .load_shapefile(path)
            .map(|loaded| loaded.frame.schema().to_vec()),
        _ => io
            .read_path(path)
            .map_err(DataError::Io)
            .and_then(|bytes| read_schema_bytes_as(bytes.as_slice(), format, sample_size)),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

/// Load every valid named table in a chart.
pub fn load_named_tables(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Result<Vec<NamedTable>, DriverError> {
    load_named_tables_with_io(chart, source, base_dir, &OsDriverIo)
}

/// Load every valid named table in a chart through an injected I/O provider.
pub fn load_named_tables_with_io(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    io: &dyn DriverIo,
) -> Result<Vec<NamedTable>, DriverError> {
    let resolved = DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_named_table_sources(chart);
    load_resolved_named_tables_with_io(resolved, io)
}

pub(crate) fn load_resolved_named_tables_with_io(
    resolved_tables: Vec<ResolvedTableSource>,
    io: &dyn DriverIo,
) -> Result<Vec<NamedTable>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolved_tables {
        let loaded = load_path_with_io(
            &resolved.path,
            resolved.format,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
            io,
        )?;
        out.push(NamedTable {
            name: resolved.name,
            path: resolved.path,
            frame: loaded.frame,
            warnings: loaded.warnings,
        });
    }
    Ok(out)
}

/// Load every valid named table schema in a chart.
pub fn load_named_table_schemas(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    sample_size: usize,
) -> Result<Vec<NamedTableSchema>, DriverError> {
    load_named_table_schemas_with_io(chart, source, base_dir, sample_size, &OsDriverIo)
}

/// Load every valid named table schema through an injected I/O provider.
pub fn load_named_table_schemas_with_io(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    sample_size: usize,
    io: &dyn DriverIo,
) -> Result<Vec<NamedTableSchema>, DriverError> {
    let resolved = DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_named_table_sources(chart);
    load_resolved_named_table_schemas_with_io(resolved, sample_size, io)
}

fn load_resolved_named_table_schemas_with_io(
    resolved_tables: Vec<ResolvedTableSource>,
    sample_size: usize,
    io: &dyn DriverIo,
) -> Result<Vec<NamedTableSchema>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolved_tables {
        let schema = load_schema_path_with_io(
            &resolved.path,
            resolved.format,
            sample_size,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
            io,
        )?;
        out.push(NamedTableSchema {
            name: resolved.name,
            path: resolved.path,
            schema,
        });
    }
    Ok(out)
}

fn read_stdin_csv(io: &dyn DriverIo) -> Result<LoadResult, DriverError> {
    let bytes = io
        .read_stdin()
        .map_err(|e| DriverError::StdinRead(format!("failed to read CSV from stdin: {e}")))?;
    read_csv(bytes.as_slice())
        .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
}

fn read_stdin_csv_schema(
    sample_size: usize,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    let bytes = io
        .read_stdin()
        .map_err(|e| DriverError::StdinRead(format!("failed to read CSV from stdin: {e}")))?;
    read_csv_schema(bytes.as_slice(), sample_size)
        .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
}
