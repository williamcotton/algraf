use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{
    validate_temporal_format, ColumnDef, DataFrame, EpochUnit, Format, LoadResult, Table,
    TemporalColumnParse, TemporalParsePolicy, TemporalParseType, TemporalTimezone,
};
use algraf_semantics::{analyze_chart_with_tables, Analysis};
use algraf_syntax::ast::{AlgebraExpr, Arg, ChartBlock, ChartItem, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value, SourceExpr};

use crate::error::{DriverError, LoadContext};
use crate::io::{DriverIo, OsDriverIo};
use crate::loading::{
    load_path_with_policy_with_io, load_primary_with_policy_with_io, load_sqlite_with_io,
    load_topojson_with_io, NamedTable,
};
use crate::report::{driver_error_diagnostic, PreparationReport, ReportPhase};
use crate::resolution::{
    resolve_chart_inputs, DataLocation, DriverEnv, ResolvedTableSource, SourceInput,
};

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
    let parse_policy = parse_policies(chart);

    let primary = resolved
        .primary
        .map(|location| load_primary_with_policy_with_io(location, io, Some(&parse_policy.primary)))
        .transpose()?;
    let schema = primary
        .as_ref()
        .map(|loaded| loaded.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let named_tables =
        load_named_tables_with_parse_policy(resolved.named_tables, io, &parse_policy.by_table)?;
    let table_schemas: HashMap<String, Vec<ColumnDef>> = named_tables
        .iter()
        .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
        .collect();
    let mut analysis = analyze_chart_with_tables(chart, schema, &table_schemas);
    analysis
        .diagnostics
        .extend(parse_policy.diagnostics.clone());
    analysis
        .diagnostics
        .extend(unknown_parse_target_diagnostics(
            &parse_policy,
            schema,
            &table_schemas,
        ));

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
    let parse_policy = parse_policies(chart);
    report.extend(
        ReportPhase::Semantic,
        parse_policy.diagnostics.iter().cloned(),
    );

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
            match load_primary_with_policy_with_io(location, io, Some(&parse_policy.primary)) {
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
        let loaded = if resolved_table.format == Some(Format::TopoJson) {
            load_topojson_with_io(
                &resolved_table.path,
                resolved_table.object.as_deref(),
                context.clone(),
                io,
            )
        } else if let Some(query) = resolved_table.query.as_deref() {
            load_sqlite_with_io(&resolved_table.path, query, context.clone(), io)
        } else {
            load_path_with_policy_with_io(
                &resolved_table.path,
                resolved_table.format,
                context.clone(),
                io,
                parse_policy.by_table.get(&resolved_table.name),
            )
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
    let mut analysis = analyze_chart_with_tables(chart, schema, &table_schemas);
    analysis
        .diagnostics
        .extend(unknown_parse_target_diagnostics(
            &parse_policy,
            schema,
            &table_schemas,
        ));
    report.extend(ReportPhase::Semantic, analysis.diagnostics.iter().cloned());

    PreparedReport {
        source: resolved.source,
        primary,
        named_tables,
        analysis,
        report,
    }
}

#[derive(Debug, Clone, Default)]
struct ChartParsePolicies {
    primary: TemporalParsePolicy,
    by_table: HashMap<String, TemporalParsePolicy>,
    diagnostics: Vec<Diagnostic>,
}

fn parse_policies(chart: &ChartBlock) -> ChartParsePolicies {
    let table_names: HashSet<String> = chart
        .items()
        .into_iter()
        .filter_map(|item| match item {
            ChartItem::Table(table) => table.name(),
            _ => None,
        })
        .collect();

    let mut out = ChartParsePolicies::default();
    let mut seen = HashSet::new();
    for item in chart.items() {
        let ChartItem::Parse(decl) = item else {
            continue;
        };
        let mut table = None;
        let mut column = None;
        let mut as_type = None;
        let mut format = None;
        let mut formats = None;
        let mut unit = None;
        let mut timezone = TemporalTimezone::Utc;

        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            match key.as_str() {
                "table" => table = bare_name_arg(&arg),
                "column" => column = bare_name_arg(&arg),
                "as" => as_type = string_arg(&arg).and_then(parse_as_type),
                "format" => format = string_arg(&arg),
                "formats" => formats = string_array_arg(&arg),
                "unit" => unit = string_arg(&arg).and_then(parse_epoch_unit),
                "timezone" => {
                    if let Some(text) = string_arg(&arg) {
                        match parse_timezone(&text) {
                            Some(tz) => timezone = tz,
                            None => out.diagnostics.push(Diagnostic::error(
                                codes::E1014,
                                format!("invalid temporal parse timezone `{text}`"),
                                node_span(arg.syntax()),
                            )),
                        }
                    }
                }
                _ => out.diagnostics.push(Diagnostic::error(
                    codes::E1014,
                    format!("unsupported Parse argument `{key}`"),
                    node_span(arg.syntax()),
                )),
            }
        }

        let Some(column) = column else {
            out.diagnostics.push(Diagnostic::error(
                codes::E1014,
                "`Parse(...)` requires `column:`",
                node_span(decl.syntax()),
            ));
            continue;
        };
        let as_type = as_type.unwrap_or(TemporalParseType::DateTime);
        if let Some(table) = &table {
            if !table_names.contains(table) {
                out.diagnostics.push(Diagnostic::error(
                    codes::E1016,
                    format!("unknown Parse target table `{table}`"),
                    node_span(decl.syntax()),
                ));
                continue;
            }
        }

        let patterns = match (format, formats, unit) {
            (Some(one), None, None) => vec![one],
            (None, Some(many), None) => many,
            (None, None, Some(_)) => Vec::new(),
            (None, None, None) => {
                out.diagnostics.push(Diagnostic::error(
                    codes::E1014,
                    "`Parse(...)` requires `format:`, `formats:`, or `unit:`",
                    node_span(decl.syntax()),
                ));
                continue;
            }
            _ => {
                out.diagnostics.push(Diagnostic::error(
                    codes::E1014,
                    "`format:`, `formats:`, and `unit:` are mutually exclusive",
                    node_span(decl.syntax()),
                ));
                continue;
            }
        };
        if patterns
            .iter()
            .any(|pattern| !validate_temporal_format(pattern))
        {
            out.diagnostics.push(Diagnostic::error(
                codes::E1014,
                "invalid temporal parse format",
                node_span(decl.syntax()),
            ));
            continue;
        }

        let key = (table.clone(), column.clone());
        if !seen.insert(key) {
            out.diagnostics.push(Diagnostic::error(
                codes::E1015,
                format!("duplicate Parse declaration for column `{column}`"),
                node_span(decl.syntax()),
            ));
            continue;
        }

        let entry = TemporalColumnParse {
            column,
            as_type,
            formats: patterns,
            unit,
            timezone,
        };
        match table {
            Some(table) => out.by_table.entry(table).or_default().columns.push(entry),
            None => out.primary.columns.push(entry),
        }
    }
    out
}

fn load_named_tables_with_parse_policy(
    resolved_tables: Vec<ResolvedTableSource>,
    io: &dyn DriverIo,
    policies: &HashMap<String, TemporalParsePolicy>,
) -> Result<Vec<NamedTable>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolved_tables {
        let context = LoadContext::Table {
            name: resolved.name.clone(),
        };
        let loaded = if resolved.format == Some(Format::TopoJson) {
            // TopoJSON currently has no policy-aware loader because properties
            // are inferred after geometry extraction.
            load_topojson_with_io(&resolved.path, resolved.object.as_deref(), context, io)?
        } else if let Some(query) = resolved.query.as_deref() {
            load_sqlite_with_io(&resolved.path, query, context, io)?
        } else {
            load_path_with_policy_with_io(
                &resolved.path,
                resolved.format,
                context,
                io,
                policies.get(&resolved.name),
            )?
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

fn unknown_parse_target_diagnostics(
    policies: &ChartParsePolicies,
    primary_schema: &[ColumnDef],
    table_schemas: &HashMap<String, Vec<ColumnDef>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for column in &policies.primary.columns {
        if !primary_schema
            .iter()
            .any(|schema| schema.name == column.column)
        {
            diagnostics.push(Diagnostic::error(
                codes::E1016,
                format!("unknown Parse target column `{}`", column.column),
                Span::new(0, 0),
            ));
        }
    }
    for (table, policy) in &policies.by_table {
        let Some(schema) = table_schemas.get(table) else {
            continue;
        };
        for column in &policy.columns {
            if !schema.iter().any(|schema| schema.name == column.column) {
                diagnostics.push(Diagnostic::error(
                    codes::E1016,
                    format!(
                        "unknown Parse target column `{}` in table `{table}`",
                        column.column
                    ),
                    Span::new(0, 0),
                ));
            }
        }
    }
    diagnostics
}

fn bare_name_arg(arg: &Arg) -> Option<String> {
    match arg.value()? {
        ValueExpr::Algebra(AlgebraExpr::Name(name)) => name.name(),
        _ => None,
    }
}

fn string_arg(arg: &Arg) -> Option<String> {
    match arg.value()? {
        ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
            Some(string_value(&lit.text().unwrap_or_default()))
        }
        _ => None,
    }
}

fn string_array_arg(arg: &Arg) -> Option<Vec<String>> {
    match arg.value()? {
        ValueExpr::Array(array) => Some(
            array
                .values()
                .into_iter()
                .filter_map(|value| match value {
                    ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
                        Some(string_value(&lit.text().unwrap_or_default()))
                    }
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn parse_as_type(value: String) -> Option<TemporalParseType> {
    match value.as_str() {
        "date" => Some(TemporalParseType::Date),
        "datetime" => Some(TemporalParseType::DateTime),
        _ => None,
    }
}

fn parse_epoch_unit(value: String) -> Option<EpochUnit> {
    match value.as_str() {
        "seconds" => Some(EpochUnit::Seconds),
        "milliseconds" => Some(EpochUnit::Milliseconds),
        "microseconds" => Some(EpochUnit::Microseconds),
        "nanoseconds" => Some(EpochUnit::Nanoseconds),
        _ => None,
    }
}

fn parse_timezone(value: &str) -> Option<TemporalTimezone> {
    if value == "UTC" {
        return Some(TemporalTimezone::Utc);
    }
    let sign = match value.as_bytes().first().copied()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hour, minute) = value[1..].split_once(':')?;
    let hour: i32 = hour.parse().ok()?;
    let minute: i32 = minute.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(TemporalTimezone::FixedOffset {
        seconds_east: sign * (hour * 3600 + minute * 60),
    })
}
