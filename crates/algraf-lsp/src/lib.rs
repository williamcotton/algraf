//! tower-lsp backend, document cache, completion, hover, and diagnostics
//! publication.
//!
//! See spec §21 (LSP architecture), §23.2 (module boundaries), and §24.2
//! (LSP pipeline).

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use algraf_core::{Diagnostic as CoreDiagnostic, Severity, Span};
use algraf_data::{read_csv_schema, ColumnDef, DataError, DataType, DEFAULT_SCHEMA_SAMPLE};
use algraf_semantics::{analyze, registry, ChartIr};
use algraf_syntax::ast::{ChartItem, LiteralKind, Root, SpaceItem, ValueExpr};
use algraf_syntax::{format, parse, tokenize, SyntaxNode};
use dashmap::DashMap;
use tokio::runtime::Runtime;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, DocumentSymbolResponse::Nested,
    Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, InsertTextFormat, Location, MarkupContent, MarkupKind, MessageType,
    NumberOrString, OneOf, Position, Range, ServerCapabilities, SymbolKind,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
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
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// The Algraf LSP backend state (spec §21.3).
pub struct Backend {
    client: Client,
    documents: Arc<DashMap<Url, DocumentState>>,
    schema_cache: Arc<DashMap<DataSourceKey, SchemaState>>,
}

impl Backend {
    pub fn new(client: Client) -> Backend {
        Backend {
            client,
            documents: Arc::new(DashMap::new()),
            schema_cache: Arc::new(DashMap::new()),
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
        let mut analysis = None;
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
                ["Scale", "Guide", "Theme"]
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
            if active_key.is_some() {
                declaration_value_items(&decl)
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
    let names: &[&str] = match decl {
        "Layout" => &["facetColumns"],
        "Guide" => &["legend", "fill", "x", "y"],
        "Theme" => &["name"],
        "Scale" => &["x", "y", "fill", "stroke"],
        _ => &[],
    };
    names
        .iter()
        .map(|name| property(name, "Declaration argument"))
        .collect()
}

fn declaration_value_items(decl: &str) -> Vec<CompletionItem> {
    match decl {
        "Theme" => ["minimal", "classic", "light", "dark", "void"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Theme name"))
            .collect(),
        "Guide" => vec![
            value_item("true", "Boolean literal"),
            value_item("false", "Boolean literal"),
            value_item("null", "Suppress a guide"),
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

const CHART_ARGS: &[&str] = &["data", "width", "height", "title", "subtitle", "caption"];
const CHART_BODY_ITEMS: &[&str] = &["Derive", "Space", "Scale", "Guide", "Theme", "Layout"];

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
        "Smooth" => "Draws a fitted smooth line over a two-dimensional space.",
        "Boxplot" => "Draws distribution summaries for grouped values.",
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
        "layout" => "Bar collision layout: `\"identity\"`, `\"stack\"`, or `\"fill\"`.",
        "stat" => "Geometry statistic option.",
        "bins" => "Histogram bin count.",
        "binWidth" => "Histogram bin width.",
        "boundary" => "Histogram bin boundary.",
        "closed" => "Histogram interval closure: `\"left\"` or `\"right\"`.",
        "xmin" => "Rectangle minimum x boundary.",
        "xmax" => "Rectangle maximum x boundary.",
        "ymin" => "Lower y boundary.",
        "ymax" => "Upper y boundary.",
        "method" => "Smooth fitting method.",
        "width" => "Geometry width setting.",
        "baseline" => "Area or bar baseline.",
        "label" => "Text label or reference-line label.",
        "anchor" => "Text anchor: `\"start\"`, `\"middle\"`, or `\"end\"`.",
        "dx" => "Horizontal text offset.",
        "dy" => "Vertical text offset.",
        "x" => "X position.",
        "y" => "Y position.",
        "xend" => "Segment end x position.",
        "yend" => "Segment end y position.",
        "sides" => "Rug sides setting.",
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
