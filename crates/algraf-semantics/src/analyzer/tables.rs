//! Named CSV table resolution (spec §10.x): `Table name = "path.csv"`.

use algraf_core::{codes, Diagnostic};
use algraf_syntax::ast::{ChartBlock, ChartItem, Root, TableDecl, ValueExpr};
use algraf_syntax::{
    is_source_constructor, node_span, source_constructor, source_expr_from_value, SourceExpr,
};

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer};
use crate::ir::TableDeclIr;

impl Analyzer<'_> {
    /// Resolve `Table name = "path.csv"` declarations into IR, registering each
    /// resolved name for later `data:` binding. Reports duplicate names (E1105)
    /// and names that conflict with a derived table (E1108). A missing/unreadable
    /// file is the caller's concern (E1106/E1107).
    pub(super) fn resolve_tables(&mut self, chart: &ChartBlock) -> Vec<TableDeclIr> {
        let mut out = Vec::new();
        let mut dup = DupGuard::new(codes::E1105, "`Table` name").related("first declared here");
        for decl in table_decls_for_chart(chart) {
            let Some(name) = decl.name() else { continue };
            let name_span = decl.name_span().unwrap_or_else(|| node_span(decl.syntax()));
            if dup.already_seen(&mut self.diagnostics, &name, name_span) {
                continue;
            }
            if self.reserved_derived_names.contains(&name) {
                self.diag(Diagnostic::error(
                    codes::E1108,
                    format!("`Table` name `{name}` conflicts with a derived table"),
                    name_span,
                ));
                continue;
            }
            dup.record(&name, name_span);

            let Some((path, query)) = self.table_source(&decl) else {
                continue;
            };
            self.table_names.insert(name.clone());
            out.push(TableDeclIr {
                name,
                path,
                query,
                span: node_span(decl.syntax()),
            });
        }
        out
    }

    /// The source path and optional query from a `Table` declaration.
    fn table_source(&mut self, decl: &TableDecl) -> Option<(String, Option<String>)> {
        match source_expr_from_value(decl.source(), false) {
            SourceExpr::Path { path, .. } => Some((path, None)),
            SourceExpr::Sqlite { path, query, .. } => Some((path, Some(query))),
            // The TopoJSON object is re-extracted from the AST by the driver at
            // load time; the table IR only records the table's existence and path.
            SourceExpr::TopoJson { path, .. } => Some((path, None)),
            SourceExpr::Invalid { span } => {
                if let Some(ValueExpr::Call(call)) = decl.source() {
                    if is_source_constructor(&call) && source_constructor(&call).is_none() {
                        self.diag(Diagnostic::error(
                            codes::E1004,
                            format!(
                                "`{}` source expects string-literal arguments",
                                call.name().unwrap_or_default()
                            ),
                            span,
                        ));
                        return None;
                    }
                }
                self.diag(Diagnostic::error(
                    codes::E1004,
                    "`Table` source must be a string-literal path or a \
                     `GeoJson`/`Shapefile`/`Sqlite`/`TopoJson`/`Parquet` source constructor",
                    span,
                ));
                None
            }
            SourceExpr::Stdin { span } => {
                self.diag(Diagnostic::error(
                    codes::E1004,
                    "`Table` source must be a string-literal path or a \
                     `GeoJson`/`Shapefile`/`Sqlite`/`TopoJson`/`Parquet` source constructor",
                    span,
                ));
                None
            }
            SourceExpr::TableRef { span, .. } => {
                self.diag(Diagnostic::error(
                    codes::E1004,
                    "`Table` source must be a string-literal path or a \
                     `GeoJson`/`Shapefile`/`Sqlite`/`TopoJson`/`Parquet` source constructor",
                    span,
                ));
                None
            }
            SourceExpr::Missing => None,
        }
    }

    pub(super) fn table_active(&self, name: &str) -> ActiveTable {
        match self.table_schemas.get(name) {
            Some(schema) => ActiveTable::from_schema(schema),
            None => ActiveTable::empty(),
        }
    }
}

fn table_decls_for_chart(chart: &ChartBlock) -> Vec<TableDecl> {
    let mut decls = chart
        .syntax()
        .parent()
        .and_then(Root::cast)
        .map(|root| root.tables())
        .unwrap_or_default();
    decls.extend(chart.items().into_iter().filter_map(|item| match item {
        ChartItem::Table(decl) => Some(decl),
        _ => None,
    }));
    decls
}
