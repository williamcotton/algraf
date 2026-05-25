use std::io::Read;
use std::path::{Path, PathBuf};

use algraf_data::{
    read_csv, read_csv_schema, read_path, read_path_as, read_schema_path, read_schema_path_as,
    ColumnDef, DataError, DataFrame, DataWarning, Format, LoadResult, Table,
};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::SourceExpr;

use crate::error::{DriverError, LoadContext};
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
    let env = DriverEnv::new(source, base_dir, data_override, false);
    let location = env.resolver().data_location(source_expr)?;
    load_location(location, LoadContext::Primary)
}

pub(crate) fn load_primary(location: DataLocation) -> Result<LoadResult, DriverError> {
    load_location(location, LoadContext::Primary)
}

fn load_location(location: DataLocation, context: LoadContext) -> Result<LoadResult, DriverError> {
    match location {
        DataLocation::Path { path, format } => load_path(&path, format, context),
        DataLocation::Stdin => read_stdin_csv(),
    }
}

/// Load a full data source from a path.
pub fn load_path(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
) -> Result<LoadResult, DriverError> {
    load_from_path(path, format, context, read_path, read_path_as)
}

/// Load only a data schema, optionally sampling rows for delimited formats.
pub fn load_schema(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    sample_size: Option<usize>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, false);
    let Some(sample_size) = sample_size else {
        return Ok(load_data(source_expr, source, base_dir, data_override)?
            .frame
            .schema()
            .to_vec());
    };

    load_schema_location(env.resolver().data_location(source_expr)?, sample_size)
}

fn load_schema_location(
    location: DataLocation,
    sample_size: usize,
) -> Result<Vec<ColumnDef>, DriverError> {
    match location {
        DataLocation::Path { path, format } => {
            load_schema_path(&path, format, sample_size, LoadContext::Primary)
        }
        DataLocation::Stdin => read_stdin_csv_schema(sample_size),
    }
}

/// Load only a schema from a path.
pub fn load_schema_path(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
) -> Result<Vec<ColumnDef>, DriverError> {
    load_from_path(
        path,
        format,
        context,
        |path| read_schema_path(path, sample_size),
        |path, format| read_schema_path_as(path, format, sample_size),
    )
}

fn load_from_path<T>(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
    inferred: impl FnOnce(&Path) -> Result<T, DataError>,
    explicit: impl FnOnce(&Path, Format) -> Result<T, DataError>,
) -> Result<T, DriverError> {
    let loaded = match format {
        Some(format) => explicit(path, format),
        None => inferred(path),
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
    let resolved = DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_named_table_sources(chart);
    load_resolved_named_tables(resolved)
}

pub(crate) fn load_resolved_named_tables(
    resolved_tables: Vec<ResolvedTableSource>,
) -> Result<Vec<NamedTable>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolved_tables {
        let loaded = load_path(
            &resolved.path,
            resolved.format,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
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
    let resolved = DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_named_table_sources(chart);
    load_resolved_named_table_schemas(resolved, sample_size)
}

fn load_resolved_named_table_schemas(
    resolved_tables: Vec<ResolvedTableSource>,
    sample_size: usize,
) -> Result<Vec<NamedTableSchema>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolved_tables {
        let schema = load_schema_path(
            &resolved.path,
            resolved.format,
            sample_size,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
        )?;
        out.push(NamedTableSchema {
            name: resolved.name,
            path: resolved.path,
            schema,
        });
    }
    Ok(out)
}

fn read_stdin_csv() -> Result<LoadResult, DriverError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| DriverError::StdinRead(format!("failed to read CSV from stdin: {e}")))?;
    read_csv(bytes.as_slice())
        .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
}

fn read_stdin_csv_schema(sample_size: usize) -> Result<Vec<ColumnDef>, DriverError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| DriverError::StdinRead(format!("failed to read CSV from stdin: {e}")))?;
    read_csv_schema(bytes.as_slice(), sample_size)
        .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
}
