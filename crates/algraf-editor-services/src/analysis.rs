use std::collections::HashMap;
use std::path::Path;

use algraf_core::{codes, Diagnostic as CoreDiagnostic};
use algraf_data::{read_sample_rows_bytes_as, ColumnDef, Format, DEFAULT_SCHEMA_SAMPLE};
use algraf_driver::DriverIo;
use algraf_driver::{
    resolve_named_table_sources, resolve_schema_cached, resolve_topojson_schema_cached,
    CachedSchema, InMemorySchemaCache, LoadContext, OsDriverIo,
};
use algraf_semantics::analyze_with_tables;
use algraf_syntax::ast::Root;
use algraf_syntax::{parse, SourceExpr, SyntaxNode};
use lsp_types::Url;

use crate::document::{
    source_input_for_uri, AnalysisState, DocumentState, ParseState, SchemaResolution,
    SourcePreview, SourcePreviews, VirtualFile,
};

const SOURCE_PREVIEW_ROW_LIMIT: usize = 3;
const SOURCE_PREVIEW_MAX_BYTES: u64 = 1_048_576;

pub fn analyze_document_blocking(
    schema_cache: &InMemorySchemaCache,
    uri: &Url,
    version: i32,
    text: String,
    fallback_schema: Vec<ColumnDef>,
) -> (DocumentState, Vec<CoreDiagnostic>) {
    analyze_document_with_io(
        schema_cache,
        &OsDriverIo,
        uri,
        version,
        text,
        fallback_schema,
        HashMap::new(),
    )
}

pub fn analyze_document_with_io(
    schema_cache: &InMemorySchemaCache,
    io: &dyn DriverIo,
    uri: &Url,
    version: i32,
    text: String,
    fallback_schema: Vec<ColumnDef>,
    virtual_files: HashMap<String, VirtualFile>,
) -> (DocumentState, Vec<CoreDiagnostic>) {
    let parsed = parse(&text);
    let syntax = parsed.syntax();
    let parse_diagnostics = parsed.diagnostics().to_vec();
    let data_source = algraf_driver::extract_data_source(&syntax);
    let sql_gated_off = parse_diagnostics
        .iter()
        .any(|d| d.code == codes::E0025.as_str());
    let schema = if sql_gated_off {
        SchemaResolution::MissingOrInvalid
    } else {
        resolve_schema(schema_cache, io, uri, &syntax, &data_source)
    };
    // Resolve chart-scoped named-table schemas so column references inside
    // `Space(..., data: tableName)` resolve in the editor (spec §10.x).
    let table_schema_resolution = if sql_gated_off {
        TableSchemaResolution::default()
    } else {
        resolve_table_schemas(schema_cache, io, uri, &syntax)
    };
    let table_schemas = table_schema_resolution.schemas;
    let mut source_previews = table_schema_resolution.source_previews;
    let primary_has_external_schema_source = !matches!(schema, SchemaResolution::MissingOrInvalid);
    let has_external_schema_sources =
        primary_has_external_schema_source || table_schema_resolution.has_external_sources;

    let mut diagnostics = parse_diagnostics.clone();
    let analysis;
    let mut primary_schema = None;
    let mut data_path = None;

    match schema {
        SchemaResolution::Ready {
            schema,
            path,
            format,
        } => {
            let result = analyze_with_tables(&syntax, &schema, &table_schemas);
            diagnostics.extend(result.diagnostics.clone());
            analysis = Some(AnalysisState {
                ir: result.ir,
                diagnostics: result.diagnostics,
            });
            if let Some(path) = path.as_ref() {
                source_previews.primary = Some(sample_source_preview(
                    io,
                    path,
                    format,
                    path.display().to_string(),
                    &schema,
                ));
            }
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
            source_previews,
            data_path,
            virtual_files,
            has_external_schema_sources,
            diagnostics: diagnostics.clone(),
        },
        diagnostics,
    )
}

fn resolve_schema(
    schema_cache: &InMemorySchemaCache,
    io: &dyn DriverIo,
    uri: &Url,
    syntax: &SyntaxNode,
    data_source: &SourceExpr,
) -> SchemaResolution {
    let span = match data_source {
        SourceExpr::Path { span, .. }
        | SourceExpr::Sqlite { span, .. }
        | SourceExpr::TopoJson { span, .. }
        | SourceExpr::TableRef { span, .. } => *span,
        _ => return SchemaResolution::MissingOrInvalid,
    };
    let source_input = source_input_for_uri(uri);
    let resolved = Root::cast(syntax.clone())
        .and_then(|root| root.chart())
        .and_then(|chart| algraf_driver::resolve_chart_data_path(&chart, &source_input, None))
        .or_else(|| algraf_driver::resolve_source_expr_path(data_source, &source_input, None));
    let Some(resolved) = resolved else {
        return SchemaResolution::MissingOrInvalid;
    };
    let path = resolved.path;

    let cached = match resolved.query.as_deref() {
        Some(query) => algraf_driver::resolve_sqlite_schema_cached(
            schema_cache,
            io,
            &path,
            query,
            DEFAULT_SCHEMA_SAMPLE,
            LoadContext::Primary,
        ),
        None if resolved.format == Some(Format::TopoJson) => resolve_topojson_schema_cached(
            schema_cache,
            io,
            &path,
            resolved.object.as_deref(),
            LoadContext::Primary,
        ),
        None => resolve_schema_cached(
            schema_cache,
            io,
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
            format: resolved.format,
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
    source_previews: SourcePreviews,
    has_external_sources: bool,
}

fn resolve_table_schemas(
    schema_cache: &InMemorySchemaCache,
    io: &dyn DriverIo,
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
                    io,
                    &resolved.path,
                    query,
                    DEFAULT_SCHEMA_SAMPLE,
                    context,
                ),
                None if resolved.format == Some(Format::TopoJson) => {
                    resolve_topojson_schema_cached(
                        schema_cache,
                        io,
                        &resolved.path,
                        resolved.object.as_deref(),
                        context,
                    )
                }
                None => resolve_schema_cached(
                    schema_cache,
                    io,
                    &resolved.path,
                    resolved.format,
                    DEFAULT_SCHEMA_SAMPLE,
                    context,
                ),
            };
            if let CachedSchema::Ready(schema) = cached {
                let preview = sample_source_preview(
                    io,
                    &resolved.path,
                    resolved.format,
                    resolved.path.display().to_string(),
                    &schema,
                );
                out.source_previews
                    .tables
                    .insert(resolved.name.clone(), preview);
                out.schemas.insert(resolved.name, schema);
            }
        }
    }
    out
}

fn sample_source_preview(
    io: &dyn DriverIo,
    path: &Path,
    format: Option<Format>,
    label: String,
    schema: &[ColumnDef],
) -> SourcePreview {
    let mut preview = SourcePreview {
        label,
        schema: schema.to_vec(),
        row_headers: Vec::new(),
        rows: Vec::new(),
    };

    let format = format.unwrap_or_else(|| Format::from_path(path));
    if !matches!(format, Format::Csv | Format::Tsv) {
        return preview;
    }
    let Ok(metadata) = io.metadata(path) else {
        return preview;
    };
    if metadata.len > SOURCE_PREVIEW_MAX_BYTES {
        return preview;
    }
    let Ok(bytes) = io.read_path(path) else {
        return preview;
    };
    let Ok(Some(sample)) = read_sample_rows_bytes_as(&bytes, format, SOURCE_PREVIEW_ROW_LIMIT)
    else {
        return preview;
    };
    preview.row_headers = sample.headers;
    preview.rows = sample.rows;
    preview
}
