//! The semantic analyzer (spec §13).
//!
//! `analyze` is pure: it takes a parsed syntax tree plus the primary data
//! source schema and produces IR and diagnostics. Filesystem resolution and
//! schema loading happen at the caller's boundary (spec §23.5); schema errors
//! such as "file not found" are produced there, not here.
//!
//! Analysis is split into per-concern passes that all operate on the shared
//! [`context::Analyzer`] state:
//!
//! - [`context`] — the analysis context, value forms, and `let` scopes;
//! - [`chart`] — `Chart(...)` header, defaults, and `Layout`;
//! - [`tables`] — named CSV table declarations;
//! - [`frames`] — spaces, algebraic frames, and frame checks;
//! - [`properties`] — geometries and property type checking;
//! - [`scales`] / [`guides`] / [`themes`] — declaration validation;
//! - [`stats`] — explicit `Derive` stats;
//! - [`lowering`] — high-level geometry desugaring.

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::ColumnDef;
use algraf_syntax::ast::{ChartBlock, Root};
use algraf_syntax::{parse, SyntaxNode};

use crate::ir::ChartIr;
use context::Analyzer;

mod args;
mod chart;
mod context;
mod frames;
mod guides;
mod lowering;
mod properties;
mod scales;
mod stats;
mod tables;
mod themes;

/// The result of semantic analysis.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Analyze a parsed tree against a primary data schema (spec §13.17).
///
/// This analyzes the document's first chart block. Multi-chart documents
/// (spec §7.1) resolve each chart against its own data source, so the caller
/// (the CLI) drives per-chart analysis with [`analyze_chart`].
pub fn analyze(root: &SyntaxNode, primary_schema: &[ColumnDef]) -> Analysis {
    analyze_with_tables(root, primary_schema, &HashMap::new())
}

/// Analyze a parsed tree against a primary schema plus named-table schemas
/// (spec §10.x). `table_schemas` maps each `Table name = "..."` declaration's
/// name to its loaded CSV schema; the caller loads them at the I/O boundary.
pub fn analyze_with_tables(
    root: &SyntaxNode,
    primary_schema: &[ColumnDef],
    table_schemas: &HashMap<String, Vec<ColumnDef>>,
) -> Analysis {
    let mut analyzer = Analyzer::new(primary_schema, table_schemas);
    let ir = Root::cast(root.clone())
        .and_then(|r| r.chart())
        .and_then(|chart| analyzer.chart(&chart));
    Analysis {
        ir,
        diagnostics: analyzer.diagnostics,
    }
}

/// Analyze a single chart block against its primary data schema (spec §7.1).
/// Used to analyze each chart of a multi-chart document independently.
pub fn analyze_chart(chart: &ChartBlock, primary_schema: &[ColumnDef]) -> Analysis {
    analyze_chart_with_tables(chart, primary_schema, &HashMap::new())
}

/// Analyze a single chart block against a primary schema plus named-table
/// schemas (spec §10.x).
pub fn analyze_chart_with_tables(
    chart: &ChartBlock,
    primary_schema: &[ColumnDef],
    table_schemas: &HashMap<String, Vec<ColumnDef>>,
) -> Analysis {
    let mut analyzer = Analyzer::new(primary_schema, table_schemas);
    let ir = analyzer.chart(chart);
    Analysis {
        ir,
        diagnostics: analyzer.diagnostics,
    }
}

/// Parse `source` and analyze it, merging parse and semantic diagnostics.
pub fn analyze_source(source: &str, primary_schema: &[ColumnDef]) -> Analysis {
    let parsed = parse(source);
    let mut analysis = analyze(&parsed.syntax(), primary_schema);
    let mut diagnostics = parsed.into_diagnostics();
    diagnostics.append(&mut analysis.diagnostics);
    Analysis {
        ir: analysis.ir,
        diagnostics,
    }
}
