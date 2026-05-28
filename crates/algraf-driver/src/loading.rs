use std::path::{Path, PathBuf};

use algraf_data::{
    read_bytes_as_with_temporal_policy, read_schema_bytes_as_with_temporal_policy, read_topojson,
    ColumnDef, DataError, DataFrame, DataWarning, Format, LoadResult, Table, TemporalParsePolicy,
};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::SourceExpr;

use crate::error::{DriverError, LoadContext};
use crate::io::{AsyncDriverIo, DriverIo, OsDriverIo};
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
    data_format_override: Option<Format>,
) -> Result<LoadResult, DriverError> {
    load_data_with_io(
        source_expr,
        source,
        base_dir,
        data_override,
        data_format_override,
        &OsDriverIo,
    )
}

/// Load a full data source for a chart through an injected I/O provider.
pub fn load_data_with_io(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    data_format_override: Option<Format>,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, data_format_override, false);
    let location = env.resolver().data_location(source_expr)?;
    load_location(location, LoadContext::Primary, io, None)
}

/// Load a full data source through an async-capable I/O provider.
pub async fn load_data_with_async_io(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    data_format_override: Option<Format>,
    io: &dyn AsyncDriverIo,
) -> Result<LoadResult, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, data_format_override, false);
    let location = env.resolver().data_location(source_expr)?;
    load_location_async(location, LoadContext::Primary, io, None).await
}

pub(crate) fn load_primary_with_policy_with_io(
    location: DataLocation,
    io: &dyn DriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    load_location(location, LoadContext::Primary, io, temporal_policy)
}

fn load_location(
    location: DataLocation,
    context: LoadContext,
    io: &dyn DriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    match location {
        DataLocation::Path { path, format } => {
            load_path_with_policy_with_io(&path, format, context, io, temporal_policy)
        }
        DataLocation::Sqlite { path, query } => load_sqlite_with_io(&path, &query, context, io),
        DataLocation::TopoJson { path, object } => {
            load_topojson_with_io(&path, object.as_deref(), context, io)
        }
        DataLocation::Input { format } => {
            read_input(io, format.unwrap_or(Format::Csv), temporal_policy)
        }
    }
}

/// Load a TopoJSON source by reading its bytes and decoding the named object.
pub(crate) fn load_topojson_with_io(
    path: &Path,
    object: Option<&str>,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    io.read_path(path)
        .map_err(DataError::Io)
        .and_then(|bytes| read_topojson(bytes.as_slice(), object))
        .map_err(|source| DriverError::Data {
            context,
            path: path.to_path_buf(),
            source,
        })
}

/// Load only the schema of a TopoJSON source.
fn load_topojson_schema_with_io(
    path: &Path,
    object: Option<&str>,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_topojson_with_io(path, object, context, io).map(|loaded| loaded.frame.schema().to_vec())
}

async fn load_location_async(
    location: DataLocation,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    match location {
        DataLocation::Path { path, format } => {
            load_path_with_policy_with_async_io(&path, format, context, io, temporal_policy).await
        }
        DataLocation::Sqlite { path, query } => {
            load_sqlite_with_async_io(&path, &query, context, io).await
        }
        DataLocation::TopoJson { path, object } => io
            .read_path_async(&path)
            .await
            .map_err(DataError::Io)
            .and_then(|bytes| read_topojson(bytes.as_slice(), object.as_deref()))
            .map_err(|source| DriverError::Data {
                context,
                path: path.clone(),
                source,
            }),
        DataLocation::Input { format } => {
            read_input_async(io, format.unwrap_or(Format::Csv), temporal_policy).await
        }
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
    load_path_with_policy_with_io(path, format, context, io, None)
}

pub(crate) fn load_path_with_policy_with_io(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
    io: &dyn DriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io.load_shapefile(path),
        _ => io.read_path(path).map_err(DataError::Io).and_then(|bytes| {
            read_bytes_as_with_temporal_policy(bytes.as_slice(), format, temporal_policy)
        }),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn load_sqlite_with_io(
    path: &Path,
    query: &str,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<LoadResult, DriverError> {
    io.load_sqlite(path, query)
        .map_err(|source| DriverError::Data {
            context,
            path: path.to_path_buf(),
            source,
        })
}

/// Load a full data source from a path through an async-capable I/O provider.
pub async fn load_path_with_async_io(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
) -> Result<LoadResult, DriverError> {
    load_path_with_policy_with_async_io(path, format, context, io, None).await
}

async fn load_path_with_policy_with_async_io(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io.load_shapefile_async(path).await,
        _ => io
            .read_path_async(path)
            .await
            .map_err(DataError::Io)
            .and_then(|bytes| {
                read_bytes_as_with_temporal_policy(bytes.as_slice(), format, temporal_policy)
            }),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

async fn load_sqlite_with_async_io(
    path: &Path,
    query: &str,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
) -> Result<LoadResult, DriverError> {
    io.load_sqlite_async(path, query)
        .await
        .map_err(|source| DriverError::Data {
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
    data_format_override: Option<Format>,
    sample_size: Option<usize>,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_schema_with_io(
        source_expr,
        source,
        base_dir,
        data_override,
        data_format_override,
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
    data_format_override: Option<Format>,
    sample_size: Option<usize>,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, data_format_override, false);
    let Some(sample_size) = sample_size else {
        return Ok(load_data_with_io(
            source_expr,
            source,
            base_dir,
            data_override,
            data_format_override,
            io,
        )?
        .frame
        .schema()
        .to_vec());
    };

    load_schema_location(
        env.resolver().data_location(source_expr)?,
        sample_size,
        io,
        None,
    )
}

/// Load only a data schema through an async-capable I/O provider.
pub async fn load_schema_with_async_io(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    data_format_override: Option<Format>,
    sample_size: Option<usize>,
    io: &dyn AsyncDriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, data_format_override, false);
    let Some(sample_size) = sample_size else {
        return Ok(load_data_with_async_io(
            source_expr,
            source,
            base_dir,
            data_override,
            data_format_override,
            io,
        )
        .await?
        .frame
        .schema()
        .to_vec());
    };

    load_schema_location_async(
        env.resolver().data_location(source_expr)?,
        sample_size,
        io,
        None,
    )
    .await
}

fn load_schema_location(
    location: DataLocation,
    sample_size: usize,
    io: &dyn DriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    match location {
        DataLocation::Path { path, format } => load_schema_path_with_policy_with_io(
            &path,
            format,
            sample_size,
            LoadContext::Primary,
            io,
            temporal_policy,
        ),
        DataLocation::Sqlite { path, query } => {
            load_sqlite_schema_with_io(&path, &query, sample_size, LoadContext::Primary, io)
        }
        DataLocation::TopoJson { path, object } => {
            load_topojson_schema_with_io(&path, object.as_deref(), LoadContext::Primary, io)
        }
        DataLocation::Input { format } => read_input_schema(
            sample_size,
            io,
            format.unwrap_or(Format::Csv),
            temporal_policy,
        ),
    }
}

async fn load_schema_location_async(
    location: DataLocation,
    sample_size: usize,
    io: &dyn AsyncDriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    match location {
        DataLocation::Path { path, format } => {
            load_schema_path_with_policy_with_async_io(
                &path,
                format,
                sample_size,
                LoadContext::Primary,
                io,
                temporal_policy,
            )
            .await
        }
        DataLocation::Sqlite { path, query } => {
            load_sqlite_schema_with_async_io(&path, &query, sample_size, LoadContext::Primary, io)
                .await
        }
        DataLocation::TopoJson { path, object } => io
            .read_path_async(&path)
            .await
            .map_err(DataError::Io)
            .and_then(|bytes| read_topojson(bytes.as_slice(), object.as_deref()))
            .map(|loaded| loaded.frame.schema().to_vec())
            .map_err(|source| DriverError::Data {
                context: LoadContext::Primary,
                path: path.clone(),
                source,
            }),
        DataLocation::Input { format } => {
            read_input_schema_async(
                sample_size,
                io,
                format.unwrap_or(Format::Csv),
                temporal_policy,
            )
            .await
        }
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
    load_schema_path_with_policy_with_io(path, format, sample_size, context, io, None)
}

fn load_schema_path_with_policy_with_io(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
    io: &dyn DriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io
            .load_shapefile(path)
            .map(|loaded| loaded.frame.schema().to_vec()),
        _ => io.read_path(path).map_err(DataError::Io).and_then(|bytes| {
            read_schema_bytes_as_with_temporal_policy(
                bytes.as_slice(),
                format,
                sample_size,
                temporal_policy,
            )
        }),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn load_sqlite_schema_with_io(
    path: &Path,
    query: &str,
    sample_size: usize,
    context: LoadContext,
    io: &dyn DriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    io.load_sqlite_schema(path, query, sample_size)
        .map_err(|source| DriverError::Data {
            context,
            path: path.to_path_buf(),
            source,
        })
}

/// Load only a data schema from a path through an async-capable I/O provider.
pub async fn load_schema_path_with_async_io(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_schema_path_with_policy_with_async_io(path, format, sample_size, context, io, None).await
}

async fn load_schema_path_with_policy_with_async_io(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let format = format.unwrap_or_else(|| Format::from_path(path));
    let loaded = match format {
        Format::Shapefile => io
            .load_shapefile_async(path)
            .await
            .map(|loaded| loaded.frame.schema().to_vec()),
        _ => io
            .read_path_async(path)
            .await
            .map_err(DataError::Io)
            .and_then(|bytes| {
                read_schema_bytes_as_with_temporal_policy(
                    bytes.as_slice(),
                    format,
                    sample_size,
                    temporal_policy,
                )
            }),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

async fn load_sqlite_schema_with_async_io(
    path: &Path,
    query: &str,
    sample_size: usize,
    context: LoadContext,
    io: &dyn AsyncDriverIo,
) -> Result<Vec<ColumnDef>, DriverError> {
    io.load_sqlite_schema_async(path, query, sample_size)
        .await
        .map_err(|source| DriverError::Data {
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
    let resolved = DriverEnv::new(source, base_dir, None, None, false)
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
        let context = LoadContext::Table {
            name: resolved.name.clone(),
        };
        let loaded = if resolved.format == Some(Format::TopoJson) {
            load_topojson_with_io(&resolved.path, resolved.object.as_deref(), context, io)?
        } else if let Some(query) = resolved.query.as_deref() {
            load_sqlite_with_io(&resolved.path, query, context, io)?
        } else {
            load_path_with_io(&resolved.path, resolved.format, context, io)?
        };
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
    let resolved = DriverEnv::new(source, base_dir, None, None, false)
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
        let context = LoadContext::Table {
            name: resolved.name.clone(),
        };
        let schema = if resolved.format == Some(Format::TopoJson) {
            load_topojson_schema_with_io(&resolved.path, resolved.object.as_deref(), context, io)?
        } else if let Some(query) = resolved.query.as_deref() {
            load_sqlite_schema_with_io(&resolved.path, query, sample_size, context, io)?
        } else {
            load_schema_path_with_io(&resolved.path, resolved.format, sample_size, context, io)?
        };
        out.push(NamedTableSchema {
            name: resolved.name,
            path: resolved.path,
            schema,
        });
    }
    Ok(out)
}

fn read_input(
    io: &dyn DriverIo,
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    let bytes = io.read_stdin().map_err(|e| {
        DriverError::StdinRead(format!(
            "failed to read caller-provided {} input: {e}",
            format.as_str()
        ))
    })?;
    read_bytes_as_with_temporal_policy(bytes.as_slice(), format, temporal_policy).map_err(|e| {
        DriverError::StdinParse(format!(
            "failed to parse caller-provided {} input: {e}",
            format.as_str()
        ))
    })
}

async fn read_input_async(
    io: &dyn AsyncDriverIo,
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DriverError> {
    let bytes = io.read_stdin_async().await.map_err(|e| {
        DriverError::StdinRead(format!(
            "failed to read caller-provided {} input: {e}",
            format.as_str()
        ))
    })?;
    read_bytes_as_with_temporal_policy(bytes.as_slice(), format, temporal_policy).map_err(|e| {
        DriverError::StdinParse(format!(
            "failed to parse caller-provided {} input: {e}",
            format.as_str()
        ))
    })
}

fn read_input_schema(
    sample_size: usize,
    io: &dyn DriverIo,
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let bytes = io.read_stdin().map_err(|e| {
        DriverError::StdinRead(format!(
            "failed to read caller-provided {} input: {e}",
            format.as_str()
        ))
    })?;
    read_schema_bytes_as_with_temporal_policy(
        bytes.as_slice(),
        format,
        sample_size,
        temporal_policy,
    )
    .map_err(|e| {
        DriverError::StdinParse(format!(
            "failed to parse caller-provided {} input: {e}",
            format.as_str()
        ))
    })
}

async fn read_input_schema_async(
    sample_size: usize,
    io: &dyn AsyncDriverIo,
    format: Format,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let bytes = io.read_stdin_async().await.map_err(|e| {
        DriverError::StdinRead(format!(
            "failed to read caller-provided {} input: {e}",
            format.as_str()
        ))
    })?;
    read_schema_bytes_as_with_temporal_policy(
        bytes.as_slice(),
        format,
        sample_size,
        temporal_policy,
    )
    .map_err(|e| {
        DriverError::StdinParse(format!(
            "failed to parse caller-provided {} input: {e}",
            format.as_str()
        ))
    })
}
