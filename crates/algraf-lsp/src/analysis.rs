use std::collections::HashMap;

use algraf_core::{codes, Diagnostic as CoreDiagnostic};
use algraf_data::{ColumnDef, DEFAULT_SCHEMA_SAMPLE};
use algraf_driver::{
    resolve_named_table_sources, resolve_schema_cached, CachedSchema, InMemorySchemaCache,
    LoadContext, OsDriverIo,
};
use algraf_semantics::analyze_with_tables;
use algraf_syntax::ast::Root;
use algraf_syntax::{parse, SourceExpr, SyntaxNode};
use tower_lsp::lsp_types::Url;

use crate::document::{
    source_input_for_uri, AnalysisState, DocumentState, ParseState, SchemaResolution,
};

pub(crate) fn analyze_document_blocking(
    schema_cache: &InMemorySchemaCache,
    uri: &Url,
    version: i32,
    text: String,
    fallback_schema: Vec<ColumnDef>,
) -> (DocumentState, Vec<CoreDiagnostic>) {
    let parsed = parse(&text);
    let syntax = parsed.syntax();
    let parse_diagnostics = parsed.diagnostics().to_vec();
    let data_source = algraf_driver::extract_data_source(&syntax);
    let primary_has_external_schema_source = matches!(
        data_source,
        SourceExpr::Path { .. } | SourceExpr::Sqlite { .. }
    );
    let sql_gated_off = parse_diagnostics
        .iter()
        .any(|d| d.code == codes::E0025.as_str());
    let schema = if sql_gated_off {
        SchemaResolution::MissingOrInvalid
    } else {
        resolve_schema(schema_cache, uri, &data_source)
    };
    // Resolve chart-scoped named-table schemas so column references inside
    // `Space(..., data: tableName)` resolve in the editor (spec §10.x).
    let table_schema_resolution = if sql_gated_off {
        TableSchemaResolution::default()
    } else {
        resolve_table_schemas(schema_cache, uri, &syntax)
    };
    let table_schemas = table_schema_resolution.schemas;
    let has_external_schema_sources =
        primary_has_external_schema_source || table_schema_resolution.has_external_sources;

    let mut diagnostics = parse_diagnostics.clone();
    let analysis;
    let mut primary_schema = None;
    let mut data_path = None;

    match schema {
        SchemaResolution::Ready { schema, path } => {
            let result = analyze_with_tables(&syntax, &schema, &table_schemas);
            diagnostics.extend(result.diagnostics.clone());
            analysis = Some(AnalysisState {
                ir: result.ir,
                diagnostics: result.diagnostics,
            });
            primary_schema = Some(schema);
            data_path = path;
        }
        SchemaResolution::MissingOrInvalid => {
            let result = analyze_with_tables(&syntax, &[], &table_schemas);
            diagnostics.extend(result.diagnostics.clone());
            analysis = Some(AnalysisState {
                ir: result.ir,
                diagnostics: result.diagnostics,
            });
        }
        SchemaResolution::Unavailable { diagnostic } => {
            diagnostics.push(diagnostic);
            let result = analyze_with_tables(&syntax, &fallback_schema, &table_schemas);
            diagnostics.extend(result.diagnostics.clone());
            analysis = Some(AnalysisState {
                ir: result.ir,
                diagnostics: result.diagnostics,
            });
            primary_schema = (!fallback_schema.is_empty()).then_some(fallback_schema);
        }
    }

    (
        DocumentState {
            text,
            version,
            parse: Some(ParseState {
                diagnostics: parse_diagnostics,
            }),
            analysis,
            primary_schema,
            table_schemas,
            data_path,
            has_external_schema_sources,
            diagnostics: diagnostics.clone(),
        },
        diagnostics,
    )
}

fn resolve_schema(
    schema_cache: &InMemorySchemaCache,
    uri: &Url,
    data_source: &SourceExpr,
) -> SchemaResolution {
    let span = match data_source {
        SourceExpr::Path { span, .. } | SourceExpr::Sqlite { span, .. } => *span,
        _ => return SchemaResolution::MissingOrInvalid,
    };
    let query = match data_source {
        SourceExpr::Sqlite { query, .. } => Some(query.as_str()),
        _ => None,
    };
    let source_input = source_input_for_uri(uri);
    let Some(resolved) = algraf_driver::resolve_source_expr_path(data_source, &source_input, None)
    else {
        return SchemaResolution::MissingOrInvalid;
    };
    let path = resolved.path;

    let cached = match query {
        Some(query) => algraf_driver::resolve_sqlite_schema_cached(
            schema_cache,
            &OsDriverIo,
            &path,
            query,
            DEFAULT_SCHEMA_SAMPLE,
            LoadContext::Primary,
        ),
        None => resolve_schema_cached(
            schema_cache,
            &OsDriverIo,
            &path,
            resolved.format,
            DEFAULT_SCHEMA_SAMPLE,
            LoadContext::Primary,
        ),
    };

    match cached {
        CachedSchema::Ready(schema) => SchemaResolution::Ready {
            schema,
            path: Some(path),
        },
        CachedSchema::Error { code, message } => SchemaResolution::Unavailable {
            diagnostic: CoreDiagnostic::error(code, message, span),
        },
    }
}

/// Resolve schemas for chart-scoped `Table name = "..."` declarations in every
/// chart, reusing the shared schema cache (spec §10.9, §10.10) along the same
/// fingerprint-validated path as the primary schema. Tables whose file is
/// missing or unreadable are simply omitted; their column references then
/// resolve as unknown, mirroring a missing primary source.
#[derive(Default)]
struct TableSchemaResolution {
    schemas: HashMap<String, Vec<ColumnDef>>,
    has_external_sources: bool,
}

fn resolve_table_schemas(
    schema_cache: &InMemorySchemaCache,
    uri: &Url,
    syntax: &SyntaxNode,
) -> TableSchemaResolution {
    let mut out = TableSchemaResolution::default();
    let Some(root) = Root::cast(syntax.clone()) else {
        return out;
    };
    let source_input = source_input_for_uri(uri);
    for chart in root.charts() {
        for resolved in resolve_named_table_sources(&chart, &source_input, None) {
            out.has_external_sources = true;
            let context = LoadContext::Table {
                name: resolved.name.clone(),
            };
            let cached = match resolved.query.as_deref() {
                Some(query) => algraf_driver::resolve_sqlite_schema_cached(
                    schema_cache,
                    &OsDriverIo,
                    &resolved.path,
                    query,
                    DEFAULT_SCHEMA_SAMPLE,
                    context,
                ),
                None => resolve_schema_cached(
                    schema_cache,
                    &OsDriverIo,
                    &resolved.path,
                    resolved.format,
                    DEFAULT_SCHEMA_SAMPLE,
                    context,
                ),
            };
            if let CachedSchema::Ready(schema) = cached {
                out.schemas.insert(resolved.name, schema);
            }
        }
    }
    out
}
