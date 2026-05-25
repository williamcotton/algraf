use std::collections::HashMap;
use std::path::Path;

use algraf_data::{ColumnDef, DataFrame, LoadResult, Table};
use algraf_semantics::{analyze_chart_with_tables, Analysis};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::SourceExpr;

use crate::error::DriverError;
use crate::loading::{load_primary, load_resolved_named_tables, NamedTable};
use crate::resolution::{resolve_chart_inputs, DriverEnv, SourceInput};

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
    pub multi_chart: bool,
}

impl<'a> PrepareOptions<'a> {
    fn env(self) -> DriverEnv<'a> {
        DriverEnv::new(
            self.source_input,
            self.base_dir,
            self.data_override,
            self.multi_chart,
        )
    }
}

/// Load data and analyze a chart.
pub fn prepare_chart(
    chart: &ChartBlock,
    options: PrepareOptions<'_>,
) -> Result<PreparedChart, DriverError> {
    let resolved = resolve_chart_inputs(chart, options.env())?;

    let primary = resolved.primary.map(load_primary).transpose()?;
    let schema = primary
        .as_ref()
        .map(|loaded| loaded.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let named_tables = load_resolved_named_tables(resolved.named_tables)?;
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
