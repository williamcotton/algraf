use std::sync::Arc;

use algraf_driver::InMemorySchemaCache;
use algraf_editor_services::analysis::analyze_document_blocking;
use algraf_editor_services::code_actions::code_actions_for;
use algraf_editor_services::completion::{completion_context, completion_items};
use algraf_editor_services::diagnostics::diagnostic_to_lsp;
use algraf_editor_services::document::DocumentState;
use algraf_editor_services::hover::hover_at;
use algraf_editor_services::inlay::inlay_hints_for;
use algraf_editor_services::navigation::{
    definition_at, reference_sites, rename_edits, renameable_at,
};
use algraf_editor_services::positions::{offset_to_position, position_to_offset, span_to_range};
use algraf_editor_services::semantic_tokens::{semantic_tokens_for, semantic_tokens_legend};
use algraf_editor_services::signature::signature_help_at;
use algraf_editor_services::symbols::document_symbols;
use algraf_syntax::{format, parse};
use dashmap::DashMap;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentRangeFormattingParams, DocumentSymbolParams, DocumentSymbolResponse,
    DocumentSymbolResponse::Nested, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    InlayHint, InlayHintParams, Location, MessageType, OneOf, Position, PrepareRenameResponse,
    Range, ReferenceParams, RenameOptions, RenameParams, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

/// The Algraf LSP backend state (spec §21.3).
pub struct Backend {
    pub(crate) client: Client,
    pub(crate) documents: Arc<DashMap<Url, DocumentState>>,
    /// Shared, fingerprint-validated schema cache owned by the driver
    /// (spec §10.9). Primary and named-table schema resolution use one policy.
    pub(crate) schema_cache: Arc<InMemorySchemaCache>,
    /// Per-document preview request counter. A newer request supersedes older
    /// in-flight preview tasks for the same document (spec §21.13, §21.18).
    pub(crate) preview_generations: Arc<DashMap<Url, u64>>,
}

impl Backend {
    pub fn new(client: Client) -> Backend {
        Backend {
            client,
            documents: Arc::new(DashMap::new()),
            schema_cache: Arc::new(InMemorySchemaCache::new()),
            preview_generations: Arc::new(DashMap::new()),
        }
    }

    async fn upsert_document(&self, uri: Url, version: i32, text: String) {
        if let Some(mut state) = self
            .document(&uri)
            .filter(|state| state.text == text && !state.has_external_schema_sources)
        {
            state.version = version;
            let diagnostics = state.diagnostics();
            let lsp_diagnostics = diagnostics
                .iter()
                .map(|d| diagnostic_to_lsp(&state.text, &uri, d))
                .collect();
            self.documents.insert(uri.clone(), state);
            self.client
                .publish_diagnostics(uri, lsp_diagnostics, Some(version))
                .await;
            return;
        }

        let schema_cache = Arc::clone(&self.schema_cache);
        let analysis_uri = uri.clone();
        let fallback_schema = self
            .document(&uri)
            .and_then(|state| state.primary_schema)
            .unwrap_or_default();
        let outcome = tokio::task::spawn_blocking(move || {
            analyze_document_blocking(&schema_cache, &analysis_uri, version, text, fallback_schema)
        })
        .await;
        let Ok((state, diagnostics)) = outcome else {
            self.client
                .log_message(MessageType::ERROR, "Algraf document analysis task failed")
                .await;
            return;
        };
        let lsp_diagnostics = diagnostics
            .iter()
            .map(|d| diagnostic_to_lsp(&state.text, &uri, d))
            .collect();
        self.documents.insert(uri.clone(), state);
        self.client
            .publish_diagnostics(uri, lsp_diagnostics, Some(version))
            .await;
    }

    pub(crate) fn document(&self, uri: &Url) -> Option<DocumentState> {
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
