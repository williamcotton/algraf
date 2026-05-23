//! tower-lsp backend, document cache, completion, hover, and diagnostics
//! publication.
//!
//! See spec §21 (LSP architecture), §23.2 (module boundaries), and §24.2
//! (LSP pipeline).

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use algraf_core::{Diagnostic as CoreDiagnostic, Severity, Span};
use algraf_data::{
    read_csv_path, read_csv_schema, ColumnDef, DataError, DataType, Table, DEFAULT_SCHEMA_SAMPLE,
};
use algraf_render::{render, Theme};
use algraf_semantics::{analyze, registry, ChartIr};
use algraf_syntax::ast::{
    AlgebraName, Arg, ChartItem, DeriveDecl, GeometryCall, LetDecl, LiteralKind, Root, SpaceItem,
    ValueExpr,
};
use algraf_syntax::{format, parse, tokenize, SyntaxKind, SyntaxNode};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionItemKind,
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentRangeFormattingParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    DocumentSymbolResponse::Nested, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, InsertTextFormat,
    Location, MarkupContent, MarkupKind, MessageType, NumberOrString, OneOf, ParameterInformation,
    ParameterLabel, Position, PrepareRenameResponse, Range, ReferenceParams, RenameOptions,
    RenameParams, SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, SignatureInformation, SymbolKind, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url, WorkDoneProgressOptions,
    WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Run the Algraf language server over standard input and output.
pub fn run_stdio() -> io::Result<()> {
    Runtime::new()?.block_on(serve_stdio());
    Ok(())
}

/// Serve the Algraf language server over standard input and output.
pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = build_service();
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Build the LSP service with the standard methods plus the custom
/// `algraf/preview` render request (spec §21.18).
pub fn build_service() -> (LspService<Backend>, tower_lsp::ClientSocket) {
    LspService::build(Backend::new)
        .custom_method("algraf/preview", Backend::preview)
        .finish()
}

/// The Algraf LSP backend state (spec §21.3).
pub struct Backend {
    client: Client,
    documents: Arc<DashMap<Url, DocumentState>>,
    schema_cache: Arc<DashMap<DataSourceKey, SchemaState>>,
    /// Per-document preview request counter. A newer request supersedes older
    /// in-flight preview tasks for the same document (spec §21.13, §21.18).
    preview_generations: Arc<DashMap<Url, u64>>,
}

impl Backend {
    pub fn new(client: Client) -> Backend {
        Backend {
            client,
            documents: Arc::new(DashMap::new()),
            schema_cache: Arc::new(DashMap::new()),
            preview_generations: Arc::new(DashMap::new()),
        }
    }

    async fn upsert_document(&self, uri: Url, version: i32, text: String) {
        let (state, diagnostics) = self.analyze_document(&uri, version, text);
        let lsp_diagnostics = diagnostics
            .iter()
            .map(|d| diagnostic_to_lsp(&state.text, &uri, d))
            .collect();
        self.documents.insert(uri.clone(), state);
        self.client
            .publish_diagnostics(uri, lsp_diagnostics, Some(version))
            .await;
    }

    fn analyze_document(
        &self,
        uri: &Url,
        version: i32,
        text: String,
    ) -> (DocumentState, Vec<CoreDiagnostic>) {
        let parsed = parse(&text);
        let syntax = parsed.syntax();
        let parse_diagnostics = parsed.diagnostics().to_vec();
        let data_source = extract_data_source(&syntax);
        let schema = self.resolve_schema(uri, &data_source);

        let mut diagnostics = parse_diagnostics.clone();
        let analysis;
        let mut primary_schema = None;
        let mut data_path = None;

        match schema {
            SchemaResolution::Ready { schema, path } => {
                let result = analyze(&syntax, &schema);
                diagnostics.extend(result.diagnostics.clone());
                analysis = Some(AnalysisState {
                    ir: result.ir,
                    diagnostics: result.diagnostics,
                });
                primary_schema = Some(schema);
                data_path = path;
            }
            SchemaResolution::MissingOrInvalid => {
                let result = analyze(&syntax, &[]);
                diagnostics.extend(result.diagnostics.clone());
                analysis = Some(AnalysisState {
                    ir: result.ir,
                    diagnostics: result.diagnostics,
                });
            }
            SchemaResolution::Unavailable { diagnostic } => {
                diagnostics.push(diagnostic);
                let fallback_schema = self
                    .document(uri)
                    .and_then(|state| state.primary_schema)
                    .unwrap_or_default();
                let result = analyze(&syntax, &fallback_schema);
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
                data_path,
            },
            diagnostics,
        )
    }

    fn resolve_schema(&self, uri: &Url, data_source: &AstDataSource) -> SchemaResolution {
        let AstDataSource::Path { value, span } = data_source else {
            return SchemaResolution::MissingOrInvalid;
        };
        let path = resolve_data_path(uri, value);
        let key = DataSourceKey(path.clone());

        if let Some(cached) = self.schema_cache.get(&key) {
            return match cached.value() {
                SchemaState::Ready {
                    schema,
                    provisional: _,
                } => SchemaResolution::Ready {
                    schema: schema.clone(),
                    path: Some(path),
                },
                SchemaState::Error { code, message } => SchemaResolution::Unavailable {
                    diagnostic: CoreDiagnostic::error(code, message.clone(), *span),
                },
            };
        }

        let loaded = std::fs::File::open(&path)
            .map_err(DataError::from)
            .and_then(|file| read_csv_schema(file, DEFAULT_SCHEMA_SAMPLE));

        match loaded {
            Ok(schema) => {
                self.schema_cache.insert(
                    key,
                    SchemaState::Ready {
                        schema: schema.clone(),
                        provisional: true,
                    },
                );
                SchemaResolution::Ready {
                    schema,
                    path: Some(path),
                }
            }
            Err(err) => {
                let (code, message) = schema_error(&path, &err);
                self.schema_cache.insert(
                    key,
                    SchemaState::Error {
                        code,
                        message: message.clone(),
                    },
                );
                SchemaResolution::Unavailable {
                    diagnostic: CoreDiagnostic::error(code, message, *span),
                }
            }
        }
    }

    fn document(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.get(uri).map(|entry| entry.value().clone())
    }

    /// Render an SVG preview of a document through the same pipeline as
    /// `algraf render` (spec §21.18). Rendering runs on a blocking task so it
    /// never stalls diagnostics, completion, or hover, and a per-document
    /// generation counter discards output that a newer request superseded.
    pub async fn preview(&self, params: PreviewParams) -> LspResult<PreviewResult> {
        let uri = params.uri;
        let generation = {
            let mut counter = self.preview_generations.entry(uri.clone()).or_insert(0);
            *counter += 1;
            *counter
        };

        let Some(state) = self.document(&uri) else {
            return Ok(PreviewResult::message(generation, "document is not open"));
        };

        // Resolve the data path in a scope so the `!Send` syntax tree is dropped
        // before the `.await`, keeping the preview future `Send`.
        let data_path = {
            let syntax = parse(&state.text).syntax();
            match extract_data_source(&syntax) {
                AstDataSource::Path { value, .. } => Ok(resolve_data_path(&uri, &value)),
                AstDataSource::Stdin => {
                    Err("preview does not support `stdin` data; use a CSV path")
                }
                AstDataSource::Missing => {
                    Err("chart has no data source; add Chart(data: \"file.csv\")")
                }
            }
        };
        let data_path = match data_path {
            Ok(path) => path,
            Err(message) => return Ok(PreviewResult::message(generation, message)),
        };

        // The resolved dependency paths let the client watch them and re-request
        // when the underlying data changes (spec §21.18).
        let data_paths = vec![data_path.display().to_string()];

        let text = state.text.clone();
        let render_path = data_path.clone();
        let outcome =
            tokio::task::spawn_blocking(move || render_preview(&text, &render_path)).await;

        // If a newer request bumped the counter while we rendered, this output
        // is stale; report supersession rather than returning it (spec §21.13).
        let superseded = self
            .preview_generations
            .get(&uri)
            .is_some_and(|latest| *latest != generation);
        if superseded {
            return Ok(PreviewResult::superseded(generation).with_data_paths(data_paths));
        }

        let result = match outcome {
            Ok(Ok(svg)) => PreviewResult::svg(generation, svg),
            Ok(Err(message)) => PreviewResult::message(generation, &message),
            Err(_) => PreviewResult::message(generation, "preview rendering task failed"),
        };
        Ok(result.with_data_paths(data_paths))
    }
}

/// Parameters for the `algraf/preview` custom request.
#[derive(Debug, Clone, Deserialize)]
pub struct PreviewParams {
    /// The document to render.
    pub uri: Url,
}

/// Result of the `algraf/preview` custom request.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewResult {
    /// The rendered SVG, when rendering succeeded.
    pub svg: Option<String>,
    /// A human-facing explanation when no SVG was produced.
    pub message: Option<String>,
    /// Whether a newer request superseded this one.
    pub superseded: bool,
    /// The request generation, so a client can ignore out-of-order replies.
    pub generation: u64,
    /// Resolved data dependency paths the client may watch for changes.
    #[serde(rename = "dataPaths")]
    pub data_paths: Vec<String>,
}

impl PreviewResult {
    fn svg(generation: u64, svg: String) -> PreviewResult {
        PreviewResult {
            svg: Some(svg),
            message: None,
            superseded: false,
            generation,
            data_paths: Vec::new(),
        }
    }

    fn message(generation: u64, message: &str) -> PreviewResult {
        PreviewResult {
            svg: None,
            message: Some(message.to_string()),
            superseded: false,
            generation,
            data_paths: Vec::new(),
        }
    }

    fn superseded(generation: u64) -> PreviewResult {
        PreviewResult {
            svg: None,
            message: None,
            superseded: true,
            generation,
            data_paths: Vec::new(),
        }
    }

    fn with_data_paths(mut self, data_paths: Vec<String>) -> PreviewResult {
        self.data_paths = data_paths;
        self
    }
}

/// Render a document to SVG using the full data and the shared render pipeline.
/// Returns a human-facing message on any condition that blocks rendering.
fn render_preview(source: &str, data_path: &Path) -> Result<String, String> {
    let parsed = parse(source);
    let root = parsed.syntax();
    if parsed
        .diagnostics()
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err("source has parse errors; fix them to preview".to_string());
    }

    let loaded = read_csv_path(data_path)
        .map_err(|e| format!("failed to load data {}: {e}", data_path.display()))?;
    let frame = loaded.frame;

    let analysis = analyze(&root, frame.schema());
    if analysis
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err("chart has errors; fix diagnostics to preview".to_string());
    }
    let ir = analysis
        .ir
        .ok_or_else(|| "analysis produced no chart".to_string())?;

    let theme = ir.theme.as_ref().map(Theme::from_ir).unwrap_or_default();

    let result = render(&ir, &frame, &theme, None).map_err(|e| e.to_string())?;
    Ok(result.svg)
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        "*".to_string(),
                        "/".to_string(),
                        "+".to_string(),
                        "(".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: Default::default(),
                            legend: semantic_tokens_legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR_REWRITE,
                        ]),
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: Default::default(),
                }),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Algraf language server initialized")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.upsert_document(doc.uri, doc.version, doc.text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.upsert_document(
            params.text_document.uri,
            params.text_document.version,
            change.text,
        )
        .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.preview_generations.remove(&uri);
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position.position);
        let context = completion_context(&state.text, offset);
        let items = completion_items(&state, context);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position_params.position);
        Ok(hover_at(&state, offset))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let syntax = parse(&state.text).syntax();
        Ok(Some(Nested(document_symbols(&state.text, &syntax))))
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let formatted = format(&state.text);
        if formatted == state.text {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: offset_to_position(&state.text, state.text.len()),
            },
            new_text: formatted,
        }]))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens_for(&state.text),
        })))
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(code_actions_for(&state, params)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position_params.position);
        Ok(definition_at(&state, &uri, offset))
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position.position);
        let include_decl = params.context.include_declaration;
        let Some(sites) = reference_sites(&state, offset) else {
            return Ok(None);
        };
        let locations = sites
            .into_iter()
            .filter(|site| include_decl || !site.is_decl)
            .map(|site| Location {
                uri: uri.clone(),
                range: span_to_range(&state.text, site.span),
            })
            .collect();
        Ok(Some(locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position_params.position);
        let Some(sites) = reference_sites(&state, offset) else {
            return Ok(None);
        };
        let highlights = sites
            .into_iter()
            .map(|site| DocumentHighlight {
                range: span_to_range(&state.text, site.span),
                kind: Some(if site.is_decl {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                }),
            })
            .collect();
        Ok(Some(highlights))
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position_params.position);
        Ok(signature_help_at(&state.text, offset))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        // The Algraf formatter is holistic and deterministic (spec §21.10), so a
        // range request reformats the whole document and returns one edit. This
        // keeps output stable rather than re-implementing a partial formatter.
        let formatted = format(&state.text);
        if formatted == state.text {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: offset_to_position(&state.text, state.text.len()),
            },
            new_text: formatted,
        }]))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.position);
        Ok(renameable_at(&state, offset)
            .map(|span| PrepareRenameResponse::Range(span_to_range(&state.text, span))))
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(&state.text, params.text_document_position.position);
        Ok(rename_edits(&state, &uri, offset, &params.new_name))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(inlay_hints_for(&state, params.range)))
    }
}

/// Cached document state (spec §21.3).
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub parse: Option<ParseState>,
    pub analysis: Option<AnalysisState>,
    pub primary_schema: Option<Vec<ColumnDef>>,
    pub data_path: Option<PathBuf>,
}

/// Cached parse state (spec §21.3).
#[derive(Debug, Clone)]
pub struct ParseState {
    pub diagnostics: Vec<CoreDiagnostic>,
}

/// Cached semantic analysis state (spec §21.3).
#[derive(Debug, Clone)]
pub struct AnalysisState {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<CoreDiagnostic>,
}

/// Schema cache key, currently one filesystem path (spec §10.9, §21.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataSourceKey(PathBuf);

/// Cached schema state.
#[derive(Debug, Clone)]
pub enum SchemaState {
    Ready {
        schema: Vec<ColumnDef>,
        /// LSP schema reads are bounded samples; semantic hard errors from
        /// sampled types are avoided by the analyzer's current column-existence
        /// checks and retained here for future type-hint policy.
        provisional: bool,
    },
    Error {
        code: &'static str,
        message: String,
    },
}

enum SchemaResolution {
    Ready {
        schema: Vec<ColumnDef>,
        path: Option<PathBuf>,
    },
    MissingOrInvalid,
    Unavailable {
        diagnostic: CoreDiagnostic,
    },
}

#[derive(Debug, Clone)]
enum AstDataSource {
    Path { value: String, span: Span },
    Stdin,
    Missing,
}

fn extract_data_source(root: &SyntaxNode) -> AstDataSource {
    let Some(chart) = Root::cast(root.clone()).and_then(|r| r.chart()) else {
        return AstDataSource::Missing;
    };
    for arg in chart.args() {
        if arg.key().as_deref() == Some("data") {
            return match arg.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    AstDataSource::Path {
                        value: strip_string(&lit.text().unwrap_or_default()),
                        span: lit.token_span().unwrap_or_else(|| node_span(lit.syntax())),
                    }
                }
                Some(ValueExpr::Stdin(_)) => AstDataSource::Stdin,
                _ => AstDataSource::Missing,
            };
        }
    }
    AstDataSource::Missing
}

fn resolve_data_path(uri: &Url, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return path;
    }

    uri.to_file_path()
        .ok()
        .and_then(|source| source.parent().map(|parent| parent.join(&path)))
        .unwrap_or(path)
}

fn schema_error(path: &std::path::Path, err: &DataError) -> (&'static str, String) {
    match err {
        DataError::Io(io) if io.kind() == io::ErrorKind::NotFound => {
            ("E1005", format!("data file not found: {}", path.display()))
        }
        DataError::Io(io) => (
            "E1006",
            format!("data file could not be read: {}: {io}", path.display()),
        ),
        DataError::Csv(err) => (
            "E1006",
            format!("CSV parse error in {}: {err}", path.display()),
        ),
        DataError::MissingHeader => ("E1007", format!("CSV header missing in {}", path.display())),
        DataError::DuplicateHeader(name) => (
            "E1008",
            format!("duplicate CSV column `{name}` in {}", path.display()),
        ),
    }
}

fn diagnostic_to_lsp(source: &str, uri: &Url, diagnostic: &CoreDiagnostic) -> Diagnostic {
    let related_information = diagnostic
        .related
        .iter()
        .map(|related| DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range: span_to_range(source, related.span),
            },
            message: related.message.clone(),
        })
        .collect::<Vec<_>>();

    let mut message = diagnostic.message.clone();
    if let Some(help) = &diagnostic.help {
        message.push_str("\n\nHelp: ");
        message.push_str(help);
    }

    Diagnostic {
        range: span_to_range(source, diagnostic.span),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Information => DiagnosticSeverity::INFORMATION,
            Severity::Hint => DiagnosticSeverity::HINT,
        }),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        code_description: None,
        source: Some("algraf".to_string()),
        message,
        related_information: (!related_information.is_empty()).then_some(related_information),
        tags: None,
        data: None,
    }
}

fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0;
    let mut line_start = 0;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    let character = source[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>();
    Position::new(line as u32, character as u32)
}

fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, ch) in source.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    if line != position.line {
        return source.len();
    }

    let mut utf16 = 0u32;
    for (rel, ch) in source[line_start..].char_indices() {
        if ch == '\n' {
            return line_start + rel;
        }
        if utf16 >= position.character {
            return line_start + rel;
        }
        let next = utf16 + ch.len_utf16() as u32;
        if next > position.character {
            return line_start + rel;
        }
        utf16 = next;
    }
    source.len()
}

const SEMANTIC_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::COMMENT,
];

fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: SEMANTIC_TYPES.to_vec(),
        token_modifiers: Vec::new(),
    }
}

fn semantic_tokens_for(source: &str) -> Vec<SemanticToken> {
    let lexed = tokenize(source);
    let tokens = lexed.tokens;
    let mut semantic = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for (idx, token) in tokens.iter().enumerate() {
        let Some(token_type) = semantic_token_type(&tokens, idx) else {
            continue;
        };
        let range = span_to_range(source, token.span);

        // The semantic-tokens protocol forbids tokens that span multiple lines.
        // A single-line token emits once; a multi-line block comment emits one
        // token per line covering that line's portion of the comment.
        let line_count = (range.end.line - range.start.line) as usize + 1;
        let lines: Vec<&str> = source.lines().collect();
        for line_offset in 0..line_count {
            let line = range.start.line + line_offset as u32;
            let start_char = if line_offset == 0 {
                range.start.character
            } else {
                0
            };
            let end_char = if line == range.end.line {
                range.end.character
            } else {
                lines
                    .get(line as usize)
                    .map(|l| l.chars().map(char::len_utf16).sum::<usize>() as u32)
                    .unwrap_or(start_char)
            };
            let length = end_char.saturating_sub(start_char);
            if length == 0 {
                continue;
            }
            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 {
                start_char - prev_start
            } else {
                start_char
            };
            semantic.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            });
            prev_line = line;
            prev_start = start_char;
        }
    }

    semantic
}

fn semantic_token_type(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> Option<u32> {
    use algraf_syntax::TokenKind;
    let token = &tokens[idx];
    match &token.kind {
        // The `let` keyword is a lowercase contextual keyword (spec §6.5); tag it
        // as a keyword when it begins a binding (followed by an identifier).
        TokenKind::Ident(name) if name == "let" && next_significant_is_ident(tokens, idx) => {
            Some(token_type_index(SemanticTokenType::KEYWORD))
        }
        TokenKind::Ident(_) if next_significant_is_colon_all(tokens, idx) => {
            Some(token_type_index(SemanticTokenType::PROPERTY))
        }
        TokenKind::Ident(name) if declaration_name(name) || registry::geometry(name).is_some() => {
            Some(token_type_index(SemanticTokenType::FUNCTION))
        }
        TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => {
            Some(token_type_index(SemanticTokenType::VARIABLE))
        }
        TokenKind::Star | TokenKind::Slash | TokenKind::Plus | TokenKind::Equal => {
            Some(token_type_index(SemanticTokenType::OPERATOR))
        }
        TokenKind::String(_) => Some(token_type_index(SemanticTokenType::STRING)),
        TokenKind::Number(_) => Some(token_type_index(SemanticTokenType::NUMBER)),
        TokenKind::True | TokenKind::False | TokenKind::Null => {
            Some(token_type_index(SemanticTokenType::KEYWORD))
        }
        TokenKind::Comment(_) => Some(token_type_index(SemanticTokenType::COMMENT)),
        _ => None,
    }
}

fn token_type_index(token_type: SemanticTokenType) -> u32 {
    SEMANTIC_TYPES
        .iter()
        .position(|candidate| *candidate == token_type)
        .unwrap_or(0) as u32
}

fn declaration_name(name: &str) -> bool {
    matches!(
        name,
        "Chart" | "Space" | "Derive" | "Scale" | "Guide" | "Theme" | "Layout" | "Bin"
    )
}

fn next_significant_is_colon_all(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment(_)))
        .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
}

fn next_significant_is_ident(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment(_)))
        .is_some_and(|token| matches!(token.kind, TokenKind::Ident(_)))
}

fn code_actions_for(state: &DocumentState, params: CodeActionParams) -> CodeActionResponse {
    let uri = params.text_document.uri;
    let mut actions = Vec::new();
    for diagnostic in params.context.diagnostics {
        let Some(code) = diagnostic_code(&diagnostic) else {
            continue;
        };
        match code {
            "H3002" => {
                if let Some(action) =
                    quote_range_action(&uri, &state.text, &diagnostic, "Quote color literal")
                {
                    actions.push(action);
                }
            }
            "E1204" if diagnostic.message.contains("expects a quoted string value") => {
                if let Some(action) =
                    quote_range_action(&uri, &state.text, &diagnostic, "Quote string option")
                {
                    actions.push(action);
                }
            }
            "E1201" => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested geometry",
                            &uri,
                            range,
                            suggestion,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            "E1101" => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested column",
                            &uri,
                            range,
                            quote_identifier_if_needed(&suggestion),
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            "E1202" => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested property",
                            &uri,
                            range,
                            suggestion,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            "E1306" => {
                if let Some((start, end)) = range_to_offsets(&state.text, diagnostic.range) {
                    let text = state.text[start..end].trim();
                    let parts: Vec<_> = text.split('*').map(str::trim).collect();
                    if parts.len() == 3 && parts.iter().all(|part| !part.is_empty()) {
                        let replacement = format!("({} * {}) / {}", parts[0], parts[1], parts[2]);
                        actions.push(edit_action(
                            "Convert third Cartesian axis to nesting",
                            &uri,
                            diagnostic.range,
                            replacement,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            "E1305" => {
                if let Some((start, end)) = range_to_offsets(&state.text, diagnostic.range) {
                    let text = state.text[start..end].trim();
                    if !text.starts_with('(') {
                        actions.push(edit_action(
                            "Parenthesize blend expression",
                            &uri,
                            diagnostic.range,
                            format!("({text})"),
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(action) = histogram_refactor_action(&state.text, &uri, params.range) {
        actions.push(action);
    }
    actions
}

/// Offer a `refactor.rewrite` that desugars a single-`Histogram` space into the
/// explicit `Derive ... = Bin(...)` plus `Rect` form the analyzer produces
/// (spec §21.12). High-confidence only: fires when the space holds exactly one
/// `Histogram` over a single-column frame and is a direct chart-body item.
fn histogram_refactor_action(source: &str, uri: &Url, range: Range) -> Option<CodeActionOrCommand> {
    let (start, end) = range_to_offsets(source, range)?;
    let syntax = parse(source).syntax();
    let root = Root::cast(syntax)?;
    let chart = root.chart()?;

    for item in chart.items() {
        let ChartItem::Space(space) = item else {
            continue;
        };
        // Only desugar a space that is a direct chart-body item, so the new
        // `Derive` can be inserted as its sibling at the same indentation.
        if space.syntax().parent().map(|p| p.kind()) != Some(SyntaxKind::CHART_BLOCK) {
            continue;
        }
        let space_span = node_span(space.syntax());
        if end < space_span.start || start > space_span.end {
            continue;
        }

        // Exactly one geometry, a Histogram, and no scale/guide/theme items.
        let mut histogram: Option<GeometryCall> = None;
        let mut other_items = false;
        for child in space.items() {
            match child {
                SpaceItem::Geometry(call) if call.name().as_deref() == Some("Histogram") => {
                    if histogram.is_some() {
                        other_items = true;
                    } else {
                        histogram = Some(call);
                    }
                }
                SpaceItem::Error(_) => {}
                _ => other_items = true,
            }
        }
        let histogram = histogram?;
        if other_items {
            return None;
        }

        // The frame must be a single column identifier (the binned vector).
        let frame = space.frame()?;
        let input = match frame {
            algraf_syntax::ast::AlgebraExpr::Name(name) => name.raw_text()?,
            _ => return None,
        };

        let bin_args = collect_arg_text(&histogram, &["bins", "binWidth", "boundary", "closed"]);
        let rect_args = collect_arg_text(&histogram, &["fill", "stroke", "strokeWidth", "alpha"]);
        let derive_name = unique_derive_name(&chart, &input);

        let bin_call = if bin_args.is_empty() {
            format!("Bin({input})")
        } else {
            format!("Bin({input}, {})", bin_args.join(", "))
        };
        let rect_props = if rect_args.is_empty() {
            String::new()
        } else {
            format!(", {}", rect_args.join(", "))
        };

        let indent = line_indent(source, space_span.start);
        let replacement = format!(
            "Derive {derive_name} = {bin_call}\n\
             {indent}Space(bin_start * count, data: {derive_name}) {{\n\
             {indent}    Rect(xmin: bin_start, xmax: bin_end, ymax: count{rect_props})\n\
             {indent}}}"
        );

        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range: span_to_range(source, space_span),
                new_text: replacement,
            }],
        );
        return Some(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Desugar Histogram into Derive + Rect".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            ..CodeAction::default()
        }));
    }
    None
}

/// Render `key: value` fragments for the named args present on a geometry call,
/// preserving the source text of each value.
fn collect_arg_text(call: &GeometryCall, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in call.args() {
        let Some(key) = arg.key() else { continue };
        if !keys.contains(&key.as_str()) {
            continue;
        }
        if let Some(value) = arg.value() {
            let text = value.syntax().text().to_string();
            out.push(format!("{key}: {}", text.trim()));
        }
    }
    out
}

/// Pick a derived-table name that does not collide with an existing `Derive`.
fn unique_derive_name(chart: &algraf_syntax::ast::ChartBlock, input: &str) -> String {
    let base_root: String = input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let base_root = base_root.trim_matches('_');
    let base = if base_root.is_empty() {
        "binned".to_string()
    } else {
        format!("{base_root}_binned")
    };
    let existing: HashSet<String> = chart
        .items()
        .into_iter()
        .filter_map(|item| match item {
            ChartItem::Derive(decl) => decl.name(),
            _ => None,
        })
        .collect();
    if !existing.contains(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The whitespace prefix of the line that `offset` falls on.
fn line_indent(source: &str, offset: usize) -> String {
    let line_start = source[..offset.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    source[line_start..offset]
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

fn diagnostic_code(diagnostic: &Diagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => Some(code.as_str()),
        NumberOrString::Number(_) => None,
    }
}

fn quote_range_action(
    uri: &Url,
    source: &str,
    diagnostic: &Diagnostic,
    title: &str,
) -> Option<CodeActionOrCommand> {
    let range = first_ident_range(source, diagnostic.range).unwrap_or(diagnostic.range);
    let (start, end) = range_to_offsets(source, range)?;
    let text = source[start..end].trim();
    if text.is_empty() || text.starts_with('"') {
        return None;
    }
    Some(edit_action(
        title,
        uri,
        range,
        format!("{text:?}"),
        diagnostic.clone(),
    ))
}

fn edit_action(
    title: &str,
    uri: &Url,
    range: Range,
    new_text: impl Into<String>,
    diagnostic: Diagnostic,
) -> CodeActionOrCommand {
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range,
            new_text: new_text.into(),
        }],
    );
    CodeActionOrCommand::CodeAction(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        ..CodeAction::default()
    })
}

fn extract_backtick_suggestion(message: &str) -> Option<String> {
    let marker = "did you mean `";
    let start = message.find(marker)? + marker.len();
    let end = message[start..].find('`')?;
    Some(message[start..start + end].to_string())
}

fn first_ident_range(source: &str, range: Range) -> Option<Range> {
    let (start, end) = range_to_offsets(source, range)?;
    tokenize(&source[start..end])
        .tokens
        .into_iter()
        .find(|token| matches!(token.kind, algraf_syntax::TokenKind::Ident(_)))
        .map(|token| {
            let span = Span::new(start + token.span.start, start + token.span.end);
            span_to_range(source, span)
        })
}

fn range_to_offsets(source: &str, range: Range) -> Option<(usize, usize)> {
    let start = position_to_offset(source, range.start);
    let end = position_to_offset(source, range.end);
    (start <= end && end <= source.len()).then_some((start, end))
}

fn node_span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

fn strip_string(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionContext {
    TopLevel,
    ChartArgs {
        active_key: Option<String>,
    },
    ChartBody,
    SpaceArgs {
        active_key: Option<String>,
        last_kind: LastTokenKind,
    },
    SpaceBody,
    GeometryArgs {
        geometry: Option<String>,
        active_key: Option<String>,
    },
    DeclArgs {
        decl: String,
        active_key: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastTokenKind {
    Operator(char),
    Identifier,
    Other,
}

fn completion_context(text: &str, offset: usize) -> CompletionContext {
    let prefix = &text[..offset.min(text.len())];
    if prefix.trim().is_empty() {
        return CompletionContext::TopLevel;
    }

    let lexed = tokenize(prefix);
    let tokens: Vec<_> = lexed
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();

    let mut calls: Vec<Option<String>> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut pending_block: Option<String> = None;
    let mut previous_ident: Option<String> = None;
    let mut call_name_stack: Vec<Option<String>> = Vec::new();

    for token in &tokens {
        use algraf_syntax::TokenKind;
        match &token.kind {
            TokenKind::Ident(name) => {
                previous_ident = Some(name.clone());
            }
            TokenKind::LParen => {
                calls.push(previous_ident.take());
                call_name_stack = calls.clone();
            }
            TokenKind::RParen => {
                if let Some(Some(name)) = calls.pop() {
                    if matches!(name.as_str(), "Chart" | "Space") {
                        pending_block = Some(name);
                    }
                }
                call_name_stack = calls.clone();
            }
            TokenKind::LBrace => {
                blocks.push(
                    pending_block
                        .take()
                        .unwrap_or_else(|| "unknown".to_string()),
                );
                previous_ident = None;
            }
            TokenKind::RBrace => {
                blocks.pop();
                previous_ident = None;
            }
            TokenKind::Whitespace | TokenKind::Comment(_) | TokenKind::Eof => {}
            _ => {
                previous_ident = None;
                if !matches!(token.kind, TokenKind::LBrace) {
                    pending_block = None;
                }
            }
        }
    }

    let active_key = active_arg_key(&tokens);
    let last_kind = last_token_kind(&tokens);
    match call_name_stack.last().and_then(|name| name.as_deref()) {
        Some("Chart") => CompletionContext::ChartArgs { active_key },
        Some("Space") => CompletionContext::SpaceArgs {
            active_key,
            last_kind,
        },
        Some("Scale" | "Guide" | "Theme" | "Layout") => CompletionContext::DeclArgs {
            decl: call_name_stack
                .last()
                .and_then(|name| name.clone())
                .unwrap_or_default(),
            active_key,
        },
        Some(name) if registry::geometry(name).is_some() => CompletionContext::GeometryArgs {
            geometry: Some(name.to_string()),
            active_key,
        },
        Some(_) => CompletionContext::GeometryArgs {
            geometry: None,
            active_key,
        },
        None => match blocks.last().map(String::as_str) {
            Some("Chart") => CompletionContext::ChartBody,
            Some("Space") => CompletionContext::SpaceBody,
            None => CompletionContext::TopLevel,
            Some(_) => CompletionContext::Unknown,
        },
    }
}

fn active_arg_key(tokens: &[algraf_syntax::TokenWithSpan]) -> Option<String> {
    use algraf_syntax::TokenKind;
    let mut segment_start = 0usize;
    for (idx, token) in tokens.iter().enumerate().rev() {
        if matches!(token.kind, TokenKind::Comma | TokenKind::LParen) {
            segment_start = idx + 1;
            break;
        }
    }
    let segment = &tokens[segment_start..];
    for window in segment.windows(2).rev() {
        if let [name, colon] = window {
            if let (TokenKind::Ident(key), TokenKind::Colon) = (&name.kind, &colon.kind) {
                return Some(key.clone());
            }
        }
    }
    None
}

fn last_token_kind(tokens: &[algraf_syntax::TokenWithSpan]) -> LastTokenKind {
    use algraf_syntax::TokenKind;
    for token in tokens.iter().rev() {
        return match token.kind {
            TokenKind::Star => LastTokenKind::Operator('*'),
            TokenKind::Slash => LastTokenKind::Operator('/'),
            TokenKind::Plus => LastTokenKind::Operator('+'),
            TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => LastTokenKind::Identifier,
            TokenKind::Eof => continue,
            _ => LastTokenKind::Other,
        };
    }
    LastTokenKind::Other
}

fn completion_items(state: &DocumentState, context: CompletionContext) -> Vec<CompletionItem> {
    match context {
        CompletionContext::TopLevel => vec![snippet(
            "Chart",
            "Chart(data: \"$1\") {\n    Space($2) {\n        Point($3)\n    }\n}",
            "Root chart block",
        )],
        CompletionContext::ChartArgs { active_key } => {
            if active_key.as_deref() == Some("data") {
                vec![
                    snippet("\"data.csv\"", "\"$1.csv\"", "CSV data path"),
                    keyword("stdin", "Read CSV data from standard input"),
                ]
            } else {
                CHART_ARGS
                    .iter()
                    .map(|name| property(name, "Chart argument"))
                    .collect()
            }
        }
        CompletionContext::ChartBody => CHART_BODY_ITEMS
            .iter()
            .map(|name| keyword(name, "Chart body item"))
            .collect(),
        CompletionContext::SpaceArgs {
            active_key,
            last_kind,
        } => {
            if active_key.as_deref() == Some("data") {
                return derived_table_items(state);
            }
            match last_kind {
                LastTokenKind::Identifier => operator_items(),
                LastTokenKind::Operator('/') => {
                    let mut categorical = column_items_matching(state, DataType::is_categorical);
                    categorical.extend(column_items_matching(state, |_| true));
                    dedupe_by_label(categorical)
                }
                LastTokenKind::Operator('*' | '+') | LastTokenKind::Other => {
                    let mut items = column_items_matching(state, |_| true);
                    items.push(keyword("(", "Start a parenthesized algebra expression"));
                    items
                }
                LastTokenKind::Operator(_) => column_items_matching(state, |_| true),
            }
        }
        CompletionContext::SpaceBody => {
            let mut items = registry::geometry_names()
                .map(|name| function(name, geometry_doc(name)))
                .collect::<Vec<_>>();
            items.extend(
                ["let", "Scale", "Guide", "Theme"]
                    .iter()
                    .map(|name| keyword(name, "Space-scoped declaration")),
            );
            items
        }
        CompletionContext::GeometryArgs {
            geometry,
            active_key,
        } => {
            if let Some(key) = active_key {
                return property_value_items(state, geometry.as_deref(), &key);
            }
            if let Some(geometry) = geometry.and_then(|name| registry::geometry(&name)) {
                geometry
                    .prop_names()
                    .map(|name| property(name, property_doc(name)))
                    .collect()
            } else {
                all_property_items()
            }
        }
        CompletionContext::DeclArgs { decl, active_key } => {
            if let Some(key) = active_key {
                declaration_value_items(state, &decl, &key)
            } else {
                declaration_arg_items(&decl)
            }
        }
        CompletionContext::Unknown => Vec::new(),
    }
}

fn derived_table_items(state: &DocumentState) -> Vec<CompletionItem> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .map(|ir| {
            ir.derived_tables
                .iter()
                .map(|table| field(&table.name, "Derived table"))
                .collect()
        })
        .unwrap_or_default()
}

fn column_items_matching(
    state: &DocumentState,
    predicate: impl Fn(DataType) -> bool,
) -> Vec<CompletionItem> {
    state
        .primary_schema
        .as_ref()
        .map(|schema| {
            schema
                .iter()
                .filter(|column| predicate(column.dtype))
                .map(column_item)
                .collect()
        })
        .unwrap_or_else(|| vec![keyword("Chart", "Schema is not available yet")])
}

fn column_item(column: &ColumnDef) -> CompletionItem {
    let insert_text = quote_identifier_if_needed(&column.name);
    CompletionItem {
        label: column.name.clone(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(format!("column: {}", dtype_name(column.dtype))),
        documentation: Some(markup(format!(
            "CSV column inferred as `{}` from a bounded LSP schema sample.",
            dtype_name(column.dtype)
        ))),
        insert_text: Some(insert_text),
        ..CompletionItem::default()
    }
}

fn property_value_items(
    state: &DocumentState,
    geometry: Option<&str>,
    property_name: &str,
) -> Vec<CompletionItem> {
    let spec = geometry
        .and_then(registry::geometry)
        .and_then(|geometry| geometry.prop(property_name));
    let mut items = Vec::new();

    let accepts_columns = match spec {
        Some(spec) => spec
            .accepts
            .iter()
            .any(|accept| matches!(accept, registry::Accept::Column)),
        None => true,
    };
    if accepts_columns {
        items.extend(column_items_matching(state, |_| true));
    }

    // `let` variables resolve in property value positions (spec §9.6).
    items.extend(variable_items(state));

    if let Some(spec) = spec {
        for accept in spec.accepts {
            match accept {
                registry::Accept::Color => {
                    items.push(color("\"#4E79A7\"", "Categorical palette blue"));
                    items.push(color("\"#E15759\"", "Categorical palette red"));
                }
                registry::Accept::Enum(values) => {
                    items.extend(
                        values
                            .iter()
                            .map(|value| value_item(&format!("\"{value}\""), "String option")),
                    );
                }
                registry::Accept::Number => items.push(value_item("1", "Number literal")),
                registry::Accept::Str => items.push(value_item("\"\"", "String literal")),
                registry::Accept::Bool => {
                    items.push(value_item("true", "Boolean literal"));
                    items.push(value_item("false", "Boolean literal"));
                }
                registry::Accept::NumberArray => {
                    items.push(value_item("[1, 2, 3]", "Number array"));
                }
                registry::Accept::Column => {}
            }
        }
    }

    dedupe_by_label(items)
}

fn declaration_arg_items(decl: &str) -> Vec<CompletionItem> {
    declaration_arg_names(decl)
        .iter()
        .map(|name| property(name, "Declaration argument"))
        .collect()
}

fn declaration_value_items(
    state: &DocumentState,
    decl: &str,
    active_key: &str,
) -> Vec<CompletionItem> {
    match (decl, active_key) {
        ("Theme", "name") => ["minimal", "classic", "light", "dark", "void"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Theme name"))
            .collect(),
        ("Guide", "axis") | ("Scale", "axis") => {
            vec![value_item("x", "X axis"), value_item("y", "Y axis")]
        }
        ("Guide", "label") => vec![value_item("\"\"", "Axis label")],
        ("Guide", "legend") | ("Guide", "grid") | ("Scale", "reverse") | ("Scale", "integer") => {
            vec![
                value_item("true", "Boolean literal"),
                value_item("false", "Boolean literal"),
            ]
        }
        ("Guide", "fill") | ("Guide", "stroke") => vec![value_item("null", "Suppress guide")],
        ("Scale", "fill") | ("Scale", "stroke") => column_items_matching(state, |_| true),
        ("Scale", "type") => vec![
            value_item("\"linear\"", "Linear scale"),
            value_item("\"log10\"", "Base-10 logarithmic scale"),
        ],
        ("Scale", "domain") => vec![value_item("[0, 1]", "Numeric domain")],
        ("Scale", "palette") => vec![
            value_item("\"default\"", "Default categorical palette"),
            value_item("\"accent\"", "Accent categorical palette"),
        ],
        ("Scale", "gradient") => {
            vec![value_item(
                "[\"#3366cc\", \"#cc3333\"]",
                "Color gradient stops",
            )]
        }
        ("Theme", "axisText") => vec![value_item(
            "Text(size: 12, fill: \"#333333\")",
            "Axis text style",
        )],
        ("Theme", "gridMajor") => vec![value_item(
            "Line(stroke: \"#dddddd\", strokeWidth: 1)",
            "Major grid line style",
        )],
        ("Theme", "background" | "plotBackground" | "axisColor" | "gridColor" | "textColor") => {
            vec![color("\"#333333\"", "Theme color")]
        }
        ("Theme", "fontSize" | "titleSize" | "pointSize" | "lineWidth") => {
            vec![value_item("12", "Number literal")]
        }
        ("Theme", "fontFamily") => vec![value_item("\"system-ui, sans-serif\"", "Font family")],
        ("Theme", "grid" | "axes") => vec![
            value_item("true", "Boolean literal"),
            value_item("false", "Boolean literal"),
        ],
        _ => Vec::new(),
    }
}

fn all_property_items() -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for geometry in registry::geometry_names().filter_map(registry::geometry) {
        for name in geometry.prop_names() {
            if seen.insert(name) {
                items.push(property(name, property_doc(name)));
            }
        }
    }
    items
}

fn operator_items() -> Vec<CompletionItem> {
    vec![
        operator("*", "Cross operator: builds Cartesian dimensions"),
        operator(
            "/",
            "Nest operator: nests the right frame inside the left frame",
        ),
        operator("+", "Blend operator: unions compatible frame domains"),
    ]
}

fn dedupe_by_label(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.label.clone()))
        .collect()
}

const CHART_ARGS: &[&str] = &[
    "data",
    "width",
    "height",
    "title",
    "subtitle",
    "caption",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
];
const CHART_BODY_ITEMS: &[&str] = &[
    "let", "Derive", "Space", "Scale", "Guide", "Theme", "Layout",
];

/// Variable-name completions for every in-document `let` binding (spec §9.6).
fn variable_items(state: &DocumentState) -> Vec<CompletionItem> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    let mut names: Vec<String> = index.lets.iter().map(|site| site.name.clone()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| field(&name, "let binding"))
        .collect()
}

fn keyword(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn function(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn property(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn field(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn value_item(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::VALUE),
        insert_text: Some(label.to_string()),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn color(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::COLOR),
        insert_text: Some(label.to_string()),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn operator(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        insert_text: Some(format!(" {label} ")),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn snippet(label: &str, insert_text: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        insert_text: Some(insert_text.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn markup(content: impl Into<String>) -> tower_lsp::lsp_types::Documentation {
    tower_lsp::lsp_types::Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: content.into(),
    })
}

fn hover_at(state: &DocumentState, offset: usize) -> Option<Hover> {
    let lexed = tokenize(&state.text);
    let tokens: Vec<_> = lexed
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let idx = tokens
        .iter()
        .position(|token| token.span.contains(offset))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.span.end == offset && token.span.start < token.span.end)
        })?;
    let token = &tokens[idx];

    use algraf_syntax::TokenKind;
    let contents = match &token.kind {
        TokenKind::Star => Some(operator_hover(
            "Cross operator",
            "`*` builds a Cartesian product between algebraic frames.",
        )),
        TokenKind::Slash => Some(operator_hover(
            "Nest operator",
            "`/` nests the right frame inside each value of the left frame.",
        )),
        TokenKind::Plus => Some(operator_hover(
            "Blend operator",
            "`+` unions compatible domains in an explicitly parenthesized blend.",
        )),
        TokenKind::Ident(name) => hover_for_ident(state, &tokens, idx, name),
        TokenKind::QuotedIdent(raw) => hover_for_ident(state, &tokens, idx, raw),
        TokenKind::String(raw) => hover_for_string(state, raw),
        _ => None,
    }?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: contents,
        }),
        range: Some(span_to_range(&state.text, token.span)),
    })
}

fn hover_for_ident(
    state: &DocumentState,
    tokens: &[algraf_syntax::TokenWithSpan],
    idx: usize,
    name: &str,
) -> Option<String> {
    if next_significant_is_colon(tokens, idx) {
        return Some(format!("**Property `{name}`**\n\n{}", property_doc(name)));
    }
    if registry::geometry(name).is_some() {
        return Some(format!("**Geometry `{name}`**\n\n{}", geometry_doc(name)));
    }
    if let Some(column) = state
        .primary_schema
        .as_ref()
        .and_then(|schema| schema.iter().find(|column| column.name == name))
    {
        let source = state
            .data_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "CSV schema sample".to_string());
        let examples = if column.examples.is_empty() {
            String::new()
        } else {
            format!("\n\nExamples: `{}`", column.examples.join("`, `"))
        };
        return Some(format!(
            "**Column `{}`**\n\nType: `{}`\n\nSource: {} (provisional LSP sample){}",
            column.name,
            dtype_name(column.dtype),
            source,
            examples
        ));
    }
    None
}

fn hover_for_string(state: &DocumentState, raw: &str) -> Option<String> {
    let value = strip_string(raw);
    if ["identity", "stack", "fill"].contains(&value.as_str()) {
        return Some(format!(
            "**Bar layout `{value}`**\n\nControls how bars resolve collisions in a shared space."
        ));
    }
    if ["lm", "loess"].contains(&value.as_str()) {
        return Some(format!(
            "**Smooth method `{value}`**\n\nControls the statistical model used for a smooth layer."
        ));
    }
    if state.data_path.is_some() {
        None
    } else {
        Some("String literal".to_string())
    }
}

fn next_significant_is_colon(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Eof))
        .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
}

fn operator_hover(title: &str, body: &str) -> String {
    format!("**{title}**\n\n{body}")
}

fn dtype_name(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}

fn geometry_doc(name: &str) -> &'static str {
    match name {
        "Point" => "Draws one point per row in the inherited space.",
        "Line" => "Draws connected line segments through row coordinates.",
        "Bar" => "Draws bars in the inherited categorical or Cartesian space.",
        "Rect" => "Draws rectangles from explicit boundary properties.",
        "Histogram" => "Bins one continuous vector and draws count bars.",
        "FreqPoly" => "Bins one continuous vector and connects bin centers.",
        "Bin2D" => "Bins two continuous dimensions into rectangles.",
        "HexBin" => "Bins two continuous dimensions into hexagons.",
        "Smooth" => "Draws a fitted smooth line over a two-dimensional space.",
        "Boxplot" => "Draws distribution summaries for grouped values.",
        "Violin" => "Draws mirrored KDE distributions per category.",
        "Ribbon" => "Draws a band between lower and upper y values.",
        "Tile" => "Draws heatmap-style tiles in a two-dimensional space.",
        "HLine" => "Draws a horizontal reference line.",
        "VLine" => "Draws a vertical reference line.",
        "Rug" => "Draws marginal tick marks for observations.",
        "Area" => "Draws a filled area from a baseline to y values.",
        "Text" => "Draws text labels in the inherited space.",
        "Segment" => "Draws explicit line segments between endpoints.",
        _ => "Algraf geometry.",
    }
}

fn property_doc(name: &str) -> &'static str {
    match name {
        "fill" => "Fill color setting or data column mapping.",
        "stroke" => "Stroke color setting or data column mapping.",
        "strokeWidth" => "Stroke width numeric setting.",
        "alpha" => "Opacity setting or data column mapping.",
        "size" => "Point or text size setting or data column mapping.",
        "shape" => "Point shape setting or data column mapping.",
        "group" => "Series grouping column, independent from color aesthetics.",
        "layout" => "Bar collision layout: `\"identity\"`, `\"stack\"`, or `\"fill\"`.",
        "stat" => "Geometry statistic option.",
        "bins" => "Histogram bin count.",
        "binWidth" => "Histogram bin width.",
        "boundary" => "Histogram bin boundary.",
        "closed" => "Histogram interval closure: `\"left\"` or `\"right\"`.",
        "bandwidth" => "Kernel density bandwidth.",
        "n" => "Number of kernel density grid points.",
        "quantiles" => "Violin quantile line positions.",
        "gradient" => "Continuous color gradient stops.",
        "xmin" => "Rectangle minimum x boundary.",
        "xmax" => "Rectangle maximum x boundary.",
        "ymin" => "Lower y boundary.",
        "ymax" => "Upper y boundary.",
        "method" => "Smooth fitting method.",
        "width" => "Geometry width setting.",
        "baseline" => "Area or bar baseline.",
        "label" => "Text label or reference-line label.",
        "anchor" => "Text anchor: `\"start\"`, `\"middle\"`, or `\"end\"`.",
        "dx" => "Horizontal text offset, in pixels: a number or a column mapping.",
        "dy" => "Vertical text offset, in pixels: a number or a column mapping.",
        "declutter" => "Spread vertically-overlapping Text labels apart (boolean).",
        "x" => "X position.",
        "y" => "Y position.",
        "xend" => "Segment end x position.",
        "yend" => "Segment end y position.",
        "sides" => "Rug sides setting.",
        "marginTop" => "Minimum top plot margin in pixels (floor over the computed margin).",
        "marginRight" => "Minimum right plot margin in pixels (floor over the computed margin).",
        "marginBottom" => "Minimum bottom plot margin in pixels (floor over the computed margin).",
        "marginLeft" => "Minimum left plot margin in pixels (floor over the computed margin).",
        _ => "Algraf argument.",
    }
}

fn document_symbols(source: &str, syntax: &SyntaxNode) -> Vec<DocumentSymbol> {
    let Some(root) = Root::cast(syntax.clone()) else {
        return Vec::new();
    };
    let Some(chart) = root.chart() else {
        return Vec::new();
    };

    let mut chart_symbol = symbol(
        source,
        "Chart",
        SymbolKind::OBJECT,
        chart.syntax(),
        Vec::new(),
    );
    let mut children = Vec::new();
    for item in chart.items() {
        match item {
            ChartItem::Derive(decl) => {
                let name = decl.name().unwrap_or_else(|| "Derive".to_string());
                children.push(symbol(
                    source,
                    &format!("Derive {name}"),
                    SymbolKind::VARIABLE,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Let(decl) => {
                let name = decl.name().unwrap_or_else(|| "let".to_string());
                children.push(symbol(
                    source,
                    &format!("let {name}"),
                    SymbolKind::VARIABLE,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Space(space) => {
                let mut space_children = Vec::new();
                for child in space.items() {
                    match child {
                        SpaceItem::Geometry(geometry) => {
                            let name = geometry.name().unwrap_or_else(|| "Geometry".to_string());
                            space_children.push(symbol(
                                source,
                                &name,
                                SymbolKind::FUNCTION,
                                geometry.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Scale(decl)
                        | SpaceItem::Guide(decl)
                        | SpaceItem::Theme(decl) => {
                            space_children.push(symbol(
                                source,
                                decl.keyword(),
                                SymbolKind::PROPERTY,
                                decl.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Let(decl) => {
                            let name = decl.name().unwrap_or_else(|| "let".to_string());
                            space_children.push(symbol(
                                source,
                                &format!("let {name}"),
                                SymbolKind::VARIABLE,
                                decl.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Error(_) => {}
                    }
                }
                children.push(symbol(
                    source,
                    "Space",
                    SymbolKind::OBJECT,
                    space.syntax(),
                    space_children,
                ));
            }
            ChartItem::Scale(decl)
            | ChartItem::Guide(decl)
            | ChartItem::Theme(decl)
            | ChartItem::Layout(decl) => {
                children.push(symbol(
                    source,
                    decl.keyword(),
                    SymbolKind::PROPERTY,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Error(_) => {}
        }
    }
    chart_symbol.children = Some(children);
    vec![chart_symbol]
}

fn symbol(
    source: &str,
    name: &str,
    kind: SymbolKind,
    node: &SyntaxNode,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    let range = span_to_range(source, node_span(node));
    #[allow(deprecated)]
    DocumentSymbol {
        name: name.to_string(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: (!children.is_empty()).then_some(children),
    }
}

fn quote_identifier_if_needed(name: &str) -> String {
    if is_plain_identifier(name) {
        return name.to_string();
    }
    let mut quoted = String::from("`");
    for ch in name.chars() {
        match ch {
            '`' => quoted.push_str("\\`"),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('`');
    quoted
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

// --- Navigation: definition, references, highlight, rename (spec §21.8) -----

/// A `Derive` declaration site within the document.
struct DeriveSite {
    name: String,
    /// Span of the table-name identifier token (the navigation target).
    name_span: Span,
}

/// A name occurrence carrying its byte span.
struct NameRef {
    name: String,
    span: Span,
}

/// A `let` declaration site, tagged with its lexical scope (spec §9.6).
struct LetSite {
    name: String,
    /// Span of the variable-name identifier token (the navigation target).
    name_span: Span,
    /// The start offset of the enclosing `Space` block, or `None` for a
    /// chart-scope binding.
    scope: Option<usize>,
}

/// A variable reference in a property value position, tagged with the scope it
/// appears in so it can be resolved against the right `let` binding.
struct VarRefSite {
    name: String,
    span: Span,
    scope: Option<usize>,
}

/// An index of all in-document name occurrences, partitioned by namespace
/// (spec §9.4). Built by walking the CST so spans are byte-accurate.
#[derive(Default)]
struct NameIndex {
    /// `Derive` declarations (derived-table definitions).
    derives: Vec<DeriveSite>,
    /// `let` declarations (variable definitions).
    lets: Vec<LetSite>,
    /// `data:` references to a derived table (e.g. `Space(..., data: binned)`).
    table_refs: Vec<NameRef>,
    /// Column references in frames, aesthetic mappings, and stat inputs.
    column_refs: Vec<NameRef>,
    /// Variable references in property value positions.
    var_refs: Vec<VarRefSite>,
}

fn build_name_index(root: &SyntaxNode) -> NameIndex {
    let mut index = NameIndex::default();

    // First pass: collect `Derive` and `let` declarations so variable
    // references can be resolved against in-scope bindings in the second pass.
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::DERIVE_DECL => {
                if let Some(decl) = DeriveDecl::cast(node.clone()) {
                    if let (Some(name), Some(span)) = (decl.name(), derive_name_span(&node)) {
                        index.derives.push(DeriveSite {
                            name,
                            name_span: span,
                        });
                    }
                }
            }
            SyntaxKind::LET_DECL => {
                if let Some(decl) = LetDecl::cast(node.clone()) {
                    if let (Some(name), Some(span)) = (decl.name(), decl.name_span()) {
                        index.lets.push(LetSite {
                            name,
                            name_span: span,
                            scope: enclosing_space_start(&node),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Second pass: classify identifier occurrences. A bare identifier in a
    // property value position that names an in-scope `let` is a variable
    // reference; otherwise it is a column reference (spec §9.6).
    for node in root.descendants() {
        if node.kind() != SyntaxKind::ALGEBRA_NAME {
            continue;
        }
        let Some(algebra) = AlgebraName::cast(node.clone()) else {
            continue;
        };
        let (Some(name), Some(span)) = (algebra.name(), algebra.ident_span()) else {
            continue;
        };
        if is_data_arg_value(&node) {
            index.table_refs.push(NameRef { name, span });
            continue;
        }
        let scope = enclosing_space_start(&node);
        if !algebra.is_quoted()
            && is_property_value(&node)
            && resolve_binding_scope(&index.lets, &name, scope).is_some()
        {
            index.var_refs.push(VarRefSite { name, span, scope });
        } else {
            index.column_refs.push(NameRef { name, span });
        }
    }
    index
}

/// The start offset of the nearest enclosing `Space` block, or `None` when the
/// node sits directly in chart scope.
fn enclosing_space_start(node: &SyntaxNode) -> Option<usize> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::SPACE_BLOCK {
            return Some(u32::from(parent.text_range().start()) as usize);
        }
        current = parent.parent();
    }
    None
}

/// Whether an `ALGEBRA_NAME` sits in a property value position (the value of an
/// argument other than `data:`), as opposed to a `Space` frame or stat input.
fn is_property_value(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            SyntaxKind::ARG => {
                return Arg::cast(parent).and_then(|arg| arg.key()).as_deref() != Some("data");
            }
            SyntaxKind::SPACE_BLOCK | SyntaxKind::STAT_CALL => return false,
            _ => {}
        }
        current = parent.parent();
    }
    false
}

/// Resolve which `let` binding a reference named `name` in scope `ref_scope`
/// binds to: a space-scope binding in the same space shadows a chart-scope one
/// (spec §9.6). Returns the binding's scope, or `None` if undefined.
fn resolve_binding_scope(
    lets: &[LetSite],
    name: &str,
    ref_scope: Option<usize>,
) -> Option<Option<usize>> {
    if let Some(space) = ref_scope {
        if lets
            .iter()
            .any(|site| site.name == name && site.scope == Some(space))
        {
            return Some(Some(space));
        }
    }
    if lets
        .iter()
        .any(|site| site.name == name && site.scope.is_none())
    {
        return Some(None);
    }
    None
}

/// The span of the table-name identifier inside a `DERIVE_DECL` node. The
/// `Derive` keyword is its own token kind, so the first `IDENT` is the name.
fn derive_name_span(node: &SyntaxNode) -> Option<Span> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| {
            let range = token.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
}

/// Whether an `ALGEBRA_NAME` node sits in the value position of a `data:`
/// argument (a derived-table reference) rather than a column position.
fn is_data_arg_value(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::ARG {
            return Arg::cast(parent).and_then(|arg| arg.key()).as_deref() == Some("data");
        }
        current = parent.parent();
    }
    false
}

/// What the identifier under the cursor refers to.
enum Target {
    DerivedTable(String),
    /// A `let` variable, identified by name and the binding's scope.
    Variable {
        name: String,
        scope: Option<usize>,
    },
    Column(String),
    /// The chart's `data:` string literal.
    DataPath,
}

fn target_at(index: &NameIndex, root: &SyntaxNode, offset: usize) -> Option<Target> {
    for derive in &index.derives {
        if derive.name_span.contains(offset) {
            return Some(Target::DerivedTable(derive.name.clone()));
        }
    }
    for site in &index.lets {
        if site.name_span.contains(offset) {
            return Some(Target::Variable {
                name: site.name.clone(),
                scope: site.scope,
            });
        }
    }
    for reference in &index.var_refs {
        if reference.span.contains(offset) {
            let scope = resolve_binding_scope(&index.lets, &reference.name, reference.scope)
                .unwrap_or(reference.scope);
            return Some(Target::Variable {
                name: reference.name.clone(),
                scope,
            });
        }
    }
    for reference in &index.table_refs {
        if reference.span.contains(offset) {
            return Some(Target::DerivedTable(reference.name.clone()));
        }
    }
    for reference in &index.column_refs {
        if reference.span.contains(offset) {
            return Some(Target::Column(reference.name.clone()));
        }
    }
    if chart_data_literal_span(root).is_some_and(|span| span.contains(offset)) {
        return Some(Target::DataPath);
    }
    None
}

/// The span of the chart-level `data:` string literal, if present.
fn chart_data_literal_span(root: &SyntaxNode) -> Option<Span> {
    let chart = Root::cast(root.clone())?.chart()?;
    for arg in chart.args() {
        if arg.key().as_deref() == Some("data") {
            if let Some(ValueExpr::Literal(literal)) = arg.value() {
                if literal.kind() == Some(LiteralKind::String) {
                    return literal.token_span();
                }
            }
        }
    }
    None
}

fn definition_at(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
) -> Option<GotoDefinitionResponse> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    match target_at(&index, &root, offset)? {
        Target::DataPath => {
            let path = state.data_path.as_ref()?;
            let target_uri = Url::from_file_path(path).ok()?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: Range::default(),
            }))
        }
        Target::Variable { name, scope } => {
            let site = index
                .lets
                .iter()
                .find(|site| site.name == name && site.scope == scope)?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_range(&state.text, site.name_span),
            }))
        }
        Target::DerivedTable(name) => {
            let site = index.derives.iter().find(|derive| derive.name == name)?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_range(&state.text, site.name_span),
            }))
        }
        Target::Column(name) => {
            let producers = derives_producing(state, &name);
            match producers.len() {
                // A derived column jumps to the `Derive` that produces it.
                1 => {
                    let site = index
                        .derives
                        .iter()
                        .find(|derive| derive.name == producers[0])?;
                    Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: span_to_range(&state.text, site.name_span),
                    }))
                }
                // Ambiguous: refuse rather than guess (spec §21.8).
                n if n > 1 => None,
                // A source column opens the CSV header (best effort).
                _ => {
                    let (path, range) = csv_header_location(state, &name)?;
                    let target_uri = Url::from_file_path(path).ok()?;
                    Some(GotoDefinitionResponse::Scalar(Location {
                        uri: target_uri,
                        range,
                    }))
                }
            }
        }
    }
}

/// Names of in-document `Derive` tables whose output schema contains `column`.
fn derives_producing(state: &DocumentState, column: &str) -> Vec<String> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .map(|ir| {
            ir.derived_tables
                .iter()
                .filter(|table| table.output_schema.iter().any(|col| col.name == column))
                .map(|table| table.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Locate a column's header within the resolved CSV file (best effort).
fn csv_header_location(state: &DocumentState, name: &str) -> Option<(PathBuf, Range)> {
    let path = state.data_path.clone()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let header = content.lines().next()?;
    let (start, end) = csv_header_field(header, name)?;
    Some((
        path,
        Range {
            start: offset_to_position(&content, start),
            end: offset_to_position(&content, end),
        },
    ))
}

/// Byte range of the header field equal to `name` in a CSV header line,
/// honoring minimal RFC-4180 double-quoting.
fn csv_header_field(header: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = header.as_bytes();
    let mut field_start = 0usize;
    let mut value = String::new();
    let mut in_quotes = false;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        match ch {
            '"' => {
                if in_quotes && bytes.get(idx + 1) == Some(&b'"') {
                    value.push('"');
                    idx += 1;
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                if value == name {
                    return Some((field_start, idx));
                }
                value.clear();
                field_start = idx + 1;
            }
            other => value.push(other),
        }
        idx += 1;
    }
    (value == name).then_some((field_start, header.len()))
}

/// A reference site for highlight/references, flagged if it is a declaration.
struct RefSite {
    span: Span,
    is_decl: bool,
}

fn reference_sites(state: &DocumentState, offset: usize) -> Option<Vec<RefSite>> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    match target_at(&index, &root, offset)? {
        Target::DataPath => None,
        Target::Variable { name, scope } => {
            let mut sites = Vec::new();
            for site in &index.lets {
                if site.name == name && site.scope == scope {
                    sites.push(RefSite {
                        span: site.name_span,
                        is_decl: true,
                    });
                }
            }
            for reference in &index.var_refs {
                if reference.name == name
                    && resolve_binding_scope(&index.lets, &reference.name, reference.scope)
                        == Some(scope)
                {
                    sites.push(RefSite {
                        span: reference.span,
                        is_decl: false,
                    });
                }
            }
            Some(sites)
        }
        Target::DerivedTable(name) => {
            let mut sites = Vec::new();
            for derive in &index.derives {
                if derive.name == name {
                    sites.push(RefSite {
                        span: derive.name_span,
                        is_decl: true,
                    });
                }
            }
            for reference in &index.table_refs {
                if reference.name == name {
                    sites.push(RefSite {
                        span: reference.span,
                        is_decl: false,
                    });
                }
            }
            Some(sites)
        }
        Target::Column(name) => Some(
            index
                .column_refs
                .iter()
                .filter(|reference| reference.name == name)
                .map(|reference| RefSite {
                    span: reference.span,
                    is_decl: false,
                })
                .collect(),
        ),
    }
}

/// The span of a renameable identifier under the cursor, if one exists. Only
/// derived-table names are user-introduced and therefore renameable.
fn renameable_at(state: &DocumentState, offset: usize) -> Option<Span> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    for derive in &index.derives {
        if derive.name_span.contains(offset) {
            return Some(derive.name_span);
        }
    }
    for reference in &index.table_refs {
        if reference.span.contains(offset) {
            return Some(reference.span);
        }
    }
    for site in &index.lets {
        if site.name_span.contains(offset) {
            return Some(site.name_span);
        }
    }
    for reference in &index.var_refs {
        if reference.span.contains(offset) {
            return Some(reference.span);
        }
    }
    None
}

fn rename_edits(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let sites = reference_sites(state, offset)?;
    if sites.is_empty() {
        return None;
    }
    let edits: Vec<TextEdit> = sites
        .into_iter()
        .map(|site| TextEdit {
            range: span_to_range(&state.text, site.span),
            new_text: new_name.to_string(),
        })
        .collect();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

// --- Signature help (spec §21.15) -------------------------------------------

/// A call/array nesting frame tracked while scanning toward the cursor.
struct CallFrame {
    /// The call name (`None` for anonymous parens and array brackets).
    name: Option<String>,
    /// Whether this frame is a `(` call rather than an array `[`.
    is_call: bool,
    /// Top-level argument separators seen so far in this frame.
    commas: usize,
}

fn signature_help_at(text: &str, offset: usize) -> Option<SignatureHelp> {
    let prefix = &text[..offset.min(text.len())];
    let tokens: Vec<_> = tokenize(prefix)
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();

    let mut stack: Vec<CallFrame> = Vec::new();
    let mut previous_ident: Option<String> = None;
    for token in &tokens {
        use algraf_syntax::TokenKind;
        match &token.kind {
            TokenKind::Ident(name) => previous_ident = Some(name.clone()),
            TokenKind::LParen => {
                stack.push(CallFrame {
                    name: previous_ident.take(),
                    is_call: true,
                    commas: 0,
                });
            }
            TokenKind::LBracket => {
                stack.push(CallFrame {
                    name: None,
                    is_call: false,
                    commas: 0,
                });
                previous_ident = None;
            }
            TokenKind::RParen | TokenKind::RBracket => {
                stack.pop();
                previous_ident = None;
            }
            TokenKind::Comma => {
                if let Some(frame) = stack.last_mut() {
                    frame.commas += 1;
                }
                previous_ident = None;
            }
            _ => previous_ident = None,
        }
    }

    let frame = stack.iter().rev().find(|frame| frame.is_call)?;
    let name = frame.name.as_deref()?;
    let params = signature_params(name)?;
    Some(build_signature(name, &params, frame.commas))
}

/// The ordered parameter names for a call, drawn from the registry and the
/// declaration metadata that also drives completion (spec §13.8–13.9).
fn signature_params(name: &str) -> Option<Vec<&'static str>> {
    if let Some(geometry) = registry::geometry(name) {
        return Some(geometry.prop_names().collect());
    }
    match name {
        "Chart" => Some(CHART_ARGS.to_vec()),
        "Scale" | "Guide" | "Theme" | "Layout" => Some(declaration_arg_names(name).to_vec()),
        "Bin" => Some(vec!["bins", "binWidth", "boundary", "closed"]),
        _ => None,
    }
}

fn build_signature(name: &str, params: &[&str], commas: usize) -> SignatureHelp {
    let mut label = format!("{name}(");
    let mut parameters = Vec::new();
    for (i, param) in params.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }
        let start = label.chars().map(char::len_utf16).sum::<usize>() as u32;
        label.push_str(param);
        let end = label.chars().map(char::len_utf16).sum::<usize>() as u32;
        parameters.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, end]),
            documentation: Some(markup(property_doc(param))),
        });
    }
    label.push(')');

    let active_parameter = if params.is_empty() {
        None
    } else {
        Some(commas.min(params.len() - 1) as u32)
    };

    SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(parameters),
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    }
}

/// The ordered argument names for a declaration keyword. Mirrors the lists
/// `declaration_arg_items` uses for completion.
fn declaration_arg_names(decl: &str) -> &'static [&'static str] {
    match decl {
        "Layout" => &["facetColumns"],
        "Guide" => &["axis", "label", "legend", "fill", "stroke", "grid"],
        "Theme" => &[
            "name",
            "axisText",
            "gridMajor",
            "fontFamily",
            "fontSize",
            "titleSize",
            "pointSize",
            "lineWidth",
            "background",
            "plotBackground",
            "axisColor",
            "gridColor",
            "textColor",
            "grid",
            "axes",
        ],
        "Scale" => &[
            "axis", "type", "domain", "reverse", "integer", "fill", "stroke", "palette",
            "gradient", "label",
        ],
        _ => &[],
    }
}

// --- Inlay hints (spec §21.17) ----------------------------------------------

/// Inlay hints showing the output columns each in-document `Derive` produces
/// (e.g. `bin_start`, `bin_end`, `bin_center`, `count`).
fn inlay_hints_for(state: &DocumentState, range: Range) -> Vec<InlayHint> {
    let Some(ir) = state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
    else {
        return Vec::new();
    };
    let (range_start, range_end) = match range_to_offsets(&state.text, range) {
        Some(offsets) => offsets,
        None => (0, state.text.len()),
    };

    let root = parse(&state.text).syntax();
    let mut hints = Vec::new();
    for node in root.descendants() {
        if node.kind() != SyntaxKind::DERIVE_DECL {
            continue;
        }
        let Some(decl) = DeriveDecl::cast(node.clone()) else {
            continue;
        };
        let Some(name) = decl.name() else { continue };
        let span = node_span(&node);
        if span.end < range_start || span.start > range_end {
            continue;
        }
        let Some(table) = ir.derived_tables.iter().find(|table| table.name == name) else {
            continue;
        };
        if table.output_schema.is_empty() {
            continue;
        }
        let columns = table
            .output_schema
            .iter()
            .map(|col| format!("{}: {}", col.name, dtype_name(col.dtype)))
            .collect::<Vec<_>>()
            .join(", ");
        hints.push(InlayHint {
            position: offset_to_position(&state.text, span.end),
            label: InlayHintLabel::String(format!(" → {columns}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        });
    }
    hints
}

#[cfg(test)]
mod semantic_token_tests {
    use super::{semantic_tokens_for, token_type_index, SemanticTokenType};

    #[test]
    fn multiline_block_comment_splits_per_line() {
        // The protocol forbids multi-line tokens: a block comment that spans
        // two lines must emit one COMMENT token per line (spec §6.10, §24).
        let source = "/* line one\n   line two */\nChart(data: \"d.csv\") {}";
        let tokens = semantic_tokens_for(source);
        let comment_type = token_type_index(SemanticTokenType::COMMENT);
        let comment_tokens = tokens
            .iter()
            .filter(|t| t.token_type == comment_type)
            .count();
        assert_eq!(comment_tokens, 2, "expected one comment token per line");
        // None of the emitted comment tokens may carry a multi-line length;
        // each is bounded by the absolute deltas the protocol requires.
        assert!(tokens.iter().all(|t| t.length > 0));
    }

    #[test]
    fn single_line_block_comment_is_one_token() {
        let source = "Chart(data: \"d.csv\") { /* note */ }";
        let tokens = semantic_tokens_for(source);
        let comment_type = token_type_index(SemanticTokenType::COMMENT);
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.token_type == comment_type)
                .count(),
            1
        );
    }
}
