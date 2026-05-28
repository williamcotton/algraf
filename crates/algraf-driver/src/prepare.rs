use std::collections::HashMap;
use std::path::Path;

use algraf_core::Span;
use algraf_data::{ColumnDef, DataFrame, Format, LoadResult, Table};
use algraf_semantics::{analyze_chart_with_tables, Analysis};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::SourceExpr;

use crate::error::{DriverError, LoadContext};
use crate::io::{DriverIo, OsDriverIo};
use crate::loading::{
    load_path_with_io, load_primary_with_io, load_resolved_named_tables_with_io,
    load_sqlite_with_io, NamedTable,
};
use crate::report::{driver_error_diagnostic, PreparationReport, ReportPhase};
use crate::resolution::{resolve_chart_inputs, DataLocation, DriverEnv, SourceInput};

/// Prepared chart inputs after loading and semantic analysis.
#[derive(Debug)]
pub struct PreparedChart {
    pub source: SourceExpr,
    pub primary: Option<LoadResult>,
    pub named_tables: Vec<NamedTable>,
    pub analysis: Analysis,
}

impl PreparedChart {
    /// Named-table schemas keyed by declaration name.
    pub fn table_schemas(&self) -> HashMap<String, Vec<ColumnDef>> {
        self.named_tables
            .iter()
            .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
            .collect()
    }

    /// Named-table frames keyed by declaration name.
    pub fn into_named_frames(self) -> HashMap<String, DataFrame> {
        self.named_tables
            .into_iter()
            .map(|table| (table.name, table.frame))
            .collect()
    }
}

/// Options for loading and analyzing one chart.
#[derive(Debug, Clone, Copy)]
pub struct PrepareOptions<'a> {
    pub source_input: &'a SourceInput,
    pub base_dir: Option<&'a Path>,
    pub data_override: Option<&'a str>,
    pub data_format_override: Option<Format>,
    pub multi_chart: bool,
}

impl<'a> PrepareOptions<'a> {
    fn env(self) -> DriverEnv<'a> {
        DriverEnv::new(
            self.source_input,
            self.base_dir,
            self.data_override,
            self.data_format_override,
            self.multi_chart,
        )
    }
}

/// Load data and analyze a chart.
pub fn prepare_chart(
    chart: &ChartBlock,
    options: PrepareOptions<'_>,
) -> Result<PreparedChart, DriverError> {
    prepare_chart_with_io(chart, options, &OsDriverIo)
}

/// Load data and analyze a chart through an injected I/O provider.
pub fn prepare_chart_with_io(
    chart: &ChartBlock,
    options: PrepareOptions<'_>,
    io: &dyn DriverIo,
) -> Result<PreparedChart, DriverError> {
    let resolved = resolve_chart_inputs(chart, options.env())?;

    let primary = resolved
        .primary
        .map(|location| load_primary_with_io(location, io))
        .transpose()?;
    let schema = primary
        .as_ref()
        .map(|loaded| loaded.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let named_tables = load_resolved_named_tables_with_io(resolved.named_tables, io)?;
    let table_schemas: HashMap<String, Vec<ColumnDef>> = named_tables
        .iter()
        .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
        .collect();
    let analysis = analyze_chart_with_tables(chart, schema, &table_schemas);

    Ok(PreparedChart {
        source: resolved.source,
        primary,
        named_tables,
        analysis,
    })
}

/// Partially prepared chart inputs: whatever could be loaded, plus a report of
/// every diagnostic and data warning observed along the way.
///
/// Unlike [`prepare_chart`], this path never short-circuits at the first
/// recoverable phase boundary. A missing or malformed data source becomes a
/// load diagnostic in the report and leaves `primary` as `None`, but semantic
/// analysis still runs against whatever schema is available — mirroring how the
/// editor inspects invalid documents (spec §21.3). Parser diagnostics are the
/// caller's responsibility to add to the report; this function fills in the
/// load and semantic phases.
#[derive(Debug)]
pub struct PreparedReport {
    pub source: SourceExpr,
    pub primary: Option<LoadResult>,
    pub named_tables: Vec<NamedTable>,
    pub analysis: Analysis,
    pub report: PreparationReport,
}

/// Load data and analyze a chart without failing at the first phase boundary.
pub fn prepare_chart_partial(chart: &ChartBlock, options: PrepareOptions<'_>) -> PreparedReport {
    prepare_chart_partial_with_io(chart, options, &OsDriverIo)
}

/// Partial preparation through an injected I/O provider (spec §23.3).
pub fn prepare_chart_partial_with_io(
    chart: &ChartBlock,
    options: PrepareOptions<'_>,
    io: &dyn DriverIo,
) -> PreparedReport {
    let mut report = PreparationReport::new();

    let resolved = match resolve_chart_inputs(chart, options.env()) {
        Ok(resolved) => resolved,
        Err(err) => {
            // Source resolution failed (for example stdin shared across charts).
            // Record it and analyze with no schema so column references still
            // resolve as far as they can.
            let source = crate::extract_chart_data_source(chart);
            let span = source.span().unwrap_or_else(|| Span::new(0, 0));
            report.push(ReportPhase::Load, driver_error_diagnostic(&err, span));
            let analysis = analyze_chart_with_tables(chart, &[], &HashMap::new());
            report.extend(ReportPhase::Semantic, analysis.diagnostics.iter().cloned());
            return PreparedReport {
                source,
                primary: None,
                named_tables: Vec::new(),
                analysis,
                report,
            };
        }
    };

    let source_span = resolved.source.span().unwrap_or_else(|| Span::new(0, 0));
    let primary = match resolved.primary {
        Some(location) => {
            let path = match &location {
                DataLocation::Path { path, .. } => Some(path.clone()),
                DataLocation::Sqlite { path, .. } => Some(path.clone()),
                DataLocation::TopoJson { path, .. } => Some(path.clone()),
                DataLocation::Input { .. } => None,
            };
            match load_primary_with_io(location, io) {
                Ok(loaded) => {
                    report.push_data_warnings(
                        &LoadContext::Primary,
                        path.as_deref(),
                        &loaded.warnings,
                    );
                    Some(loaded)
                }
                Err(err) => {
                    report.push(
                        ReportPhase::Load,
                        driver_error_diagnostic(&err, source_span),
                    );
                    None
                }
            }
        }
        None => None,
    };

    let mut named_tables = Vec::new();
    for resolved_table in resolved.named_tables {
        let context = LoadContext::Table {
            name: resolved_table.name.clone(),
        };
        let loaded = match resolved_table.query.as_deref() {
            Some(query) => load_sqlite_with_io(&resolved_table.path, query, context.clone(), io),
            None => load_path_with_io(
                &resolved_table.path,
                resolved_table.format,
                context.clone(),
                io,
            ),
        };
        match loaded {
            Ok(loaded) => {
                report.push_data_warnings(
                    &context,
                    Some(resolved_table.path.as_path()),
                    &loaded.warnings,
                );
                named_tables.push(NamedTable {
                    name: resolved_table.name,
                    path: resolved_table.path,
                    frame: loaded.frame,
                    warnings: loaded.warnings,
                });
            }
            Err(err) => {
                let span = resolved_table.span.unwrap_or(source_span);
                report.push(ReportPhase::Load, driver_error_diagnostic(&err, span));
            }
        }
    }

    let schema = primary
        .as_ref()
        .map(|loaded| loaded.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);
    let table_schemas: HashMap<String, Vec<ColumnDef>> = named_tables
        .iter()
        .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
        .collect();
    let analysis = analyze_chart_with_tables(chart, schema, &table_schemas);
    report.extend(ReportPhase::Semantic, analysis.diagnostics.iter().cloned());

    PreparedReport {
        source: resolved.source,
        primary,
        named_tables,
        analysis,
        report,
    }
}
