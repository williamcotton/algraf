use algraf_syntax::{format, parse};
use lsp_types::{
    CodeActionContext, CodeActionParams, CodeActionResponse, CompletionResponse, DocumentHighlight,
    DocumentHighlightKind, DocumentSymbolResponse::Nested, GotoDefinitionResponse, InlayHint,
    Location, Position, PrepareRenameResponse, Range, SemanticTokens, SemanticTokensResult,
    TextDocumentIdentifier, TextDocumentPositionParams, TextEdit, Url, WorkspaceEdit,
};
use serde::{Deserialize, Serialize};

use crate::code_actions::code_actions_for;
use crate::completion::{completion_context, completion_items};
use crate::diagnostics::diagnostic_to_lsp;
use crate::document::DocumentState;
use crate::hover::hover_at;
use crate::inlay::inlay_hints_for;
use crate::navigation::{definition_at, reference_sites, rename_edits, renameable_at};
use crate::positions::{offset_to_position, position_to_offset, span_to_range};
use crate::semantic_tokens::semantic_tokens_for;
use crate::signature::signature_help_at;
use crate::symbols::document_symbols;

/// Browser/native editor feature request. The shape intentionally stays close
/// to LSP request parameters while omitting JSON-RPC transport fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EditorFeatureRequest {
    Diagnostics,
    Hover {
        position: Position,
    },
    Completion {
        position: Position,
    },
    SignatureHelp {
        position: Position,
    },
    Formatting,
    RangeFormatting {
        range: Range,
    },
    SemanticTokens,
    CodeActions {
        range: Range,
        #[serde(default)]
        diagnostics: Vec<lsp_types::Diagnostic>,
    },
    Definition {
        position: Position,
    },
    References {
        position: Position,
        #[serde(default)]
        include_declaration: bool,
    },
    DocumentHighlights {
        position: Position,
    },
    PrepareRename {
        position: Position,
    },
    Rename {
        position: Position,
        new_name: String,
    },
    DocumentSymbols,
    InlayHints {
        range: Range,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct EditorFeatureResponse {
    pub diagnostics: Vec<lsp_types::Diagnostic>,
    pub result: serde_json::Value,
    pub error: Option<String>,
}

impl EditorFeatureResponse {
    pub fn ok<T: Serialize>(state: &DocumentState, uri: &Url, result: &T) -> EditorFeatureResponse {
        let result = serde_json::to_value(result).unwrap_or_else(|err| {
            serde_json::json!({
                "serializationError": err.to_string()
            })
        });
        EditorFeatureResponse {
            diagnostics: diagnostics_for(state, uri),
            result,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> EditorFeatureResponse {
        EditorFeatureResponse {
            diagnostics: Vec::new(),
            result: serde_json::Value::Null,
            error: Some(message.into()),
        }
    }
}

pub fn diagnostics_for(state: &DocumentState, uri: &Url) -> Vec<lsp_types::Diagnostic> {
    state
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic_to_lsp(&state.text, uri, diagnostic))
        .collect()
}

pub fn handle_feature_request(
    state: &DocumentState,
    uri: &Url,
    request: EditorFeatureRequest,
) -> EditorFeatureResponse {
    match request {
        EditorFeatureRequest::Diagnostics => EditorFeatureResponse::ok(state, uri, &()),
        EditorFeatureRequest::Hover { position } => {
            let offset = position_to_offset(&state.text, position);
            EditorFeatureResponse::ok(state, uri, &hover_at(state, offset))
        }
        EditorFeatureRequest::Completion { position } => {
            let offset = position_to_offset(&state.text, position);
            let context = completion_context(&state.text, offset);
            let response = Some(CompletionResponse::Array(completion_items(state, context)));
            EditorFeatureResponse::ok(state, uri, &response)
        }
        EditorFeatureRequest::SignatureHelp { position } => {
            let offset = position_to_offset(&state.text, position);
            EditorFeatureResponse::ok(state, uri, &signature_help_at(&state.text, offset))
        }
        EditorFeatureRequest::Formatting | EditorFeatureRequest::RangeFormatting { .. } => {
            EditorFeatureResponse::ok(state, uri, &formatting_edits(state))
        }
        EditorFeatureRequest::SemanticTokens => {
            let result = Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_tokens_for(&state.text),
            }));
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::CodeActions { range, diagnostics } => {
            let params = CodeActionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range,
                context: CodeActionContext {
                    diagnostics,
                    only: None,
                    trigger_kind: None,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            let actions: CodeActionResponse = code_actions_for(state, params);
            EditorFeatureResponse::ok(state, uri, &actions)
        }
        EditorFeatureRequest::Definition { position } => {
            let offset = position_to_offset(&state.text, position);
            let result: Option<GotoDefinitionResponse> = definition_at(state, uri, offset);
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::References {
            position,
            include_declaration,
        } => {
            let offset = position_to_offset(&state.text, position);
            let result: Option<Vec<Location>> = reference_sites(state, offset).map(|sites| {
                sites
                    .into_iter()
                    .filter(|site| include_declaration || !site.is_decl)
                    .map(|site| Location {
                        uri: uri.clone(),
                        range: span_to_range(&state.text, site.span),
                    })
                    .collect()
            });
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::DocumentHighlights { position } => {
            let offset = position_to_offset(&state.text, position);
            let result: Option<Vec<DocumentHighlight>> =
                reference_sites(state, offset).map(|sites| {
                    sites
                        .into_iter()
                        .map(|site| DocumentHighlight {
                            range: span_to_range(&state.text, site.span),
                            kind: Some(if site.is_decl {
                                DocumentHighlightKind::WRITE
                            } else {
                                DocumentHighlightKind::READ
                            }),
                        })
                        .collect()
                });
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::PrepareRename { position } => {
            let offset = position_to_offset(&state.text, position);
            let result: Option<PrepareRenameResponse> = renameable_at(state, offset)
                .map(|span| PrepareRenameResponse::Range(span_to_range(&state.text, span)));
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::Rename { position, new_name } => {
            let offset = position_to_offset(&state.text, position);
            let result: Option<WorkspaceEdit> = rename_edits(state, uri, offset, &new_name);
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::DocumentSymbols => {
            let syntax = parse(&state.text).syntax();
            let result = Some(Nested(document_symbols(&state.text, &syntax)));
            EditorFeatureResponse::ok(state, uri, &result)
        }
        EditorFeatureRequest::InlayHints { range } => {
            let result: Vec<InlayHint> = inlay_hints_for(state, range);
            EditorFeatureResponse::ok(state, uri, &result)
        }
    }
}

fn formatting_edits(state: &DocumentState) -> Option<Vec<TextEdit>> {
    let formatted = format(&state.text);
    if formatted == state.text {
        return Some(Vec::new());
    }
    Some(vec![TextEdit {
        range: Range {
            start: Position::new(0, 0),
            end: offset_to_position(&state.text, state.text.len()),
        },
        new_text: formatted,
    }])
}

pub fn text_document_position(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::path::Path;

    use algraf_driver::{DriverIo, DriverPathMetadata, InMemorySchemaCache};
    use lsp_types::{CompletionResponse, GotoDefinitionResponse, Hover, HoverContents};

    use super::*;
    use crate::analysis::analyze_document_with_io;
    use crate::document::VirtualFile;
    use crate::hover::hover_at;

    #[derive(Default)]
    struct TestIo {
        files: HashMap<String, Vec<u8>>,
    }

    impl DriverIo for TestIo {
        fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            self.files
                .get(name)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("missing {name}")))
        }

        fn read_stdin(&self) -> io::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
            let bytes = self.read_path(path)?;
            Ok(DriverPathMetadata {
                len: bytes.len() as u64,
                modified: None,
            })
        }
    }

    fn virtual_uri(name: &str) -> Url {
        let mut uri = Url::parse("inmemory://algraf/").unwrap();
        uri.set_path(&format!("/{name}"));
        uri
    }

    fn analyzed(source: &str, files: &[(&str, &str)]) -> (DocumentState, Url) {
        let io = TestIo {
            files: files
                .iter()
                .map(|(name, text)| ((*name).to_string(), text.as_bytes().to_vec()))
                .collect(),
        };
        let virtual_files = files
            .iter()
            .map(|(name, text)| {
                (
                    (*name).to_string(),
                    VirtualFile {
                        uri: virtual_uri(name),
                        text: (*text).to_string(),
                    },
                )
            })
            .collect();
        let uri = Url::parse("inmemory://algraf/demo.ag").unwrap();
        let cache = InMemorySchemaCache::new();
        let (state, diagnostics) = analyze_document_with_io(
            &cache,
            &io,
            &uri,
            0,
            source.to_string(),
            Vec::new(),
            virtual_files,
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != algraf_core::Severity::Error),
            "{diagnostics:?}"
        );
        (state, uri)
    }

    fn hover_markdown(hover: &Hover) -> &str {
        match &hover.contents {
            HoverContents::Markup(markup) => &markup.value,
            _ => panic!("expected markup hover"),
        }
    }

    const GRID_TOPOJSON: &str = r#"{
      "type": "Topology",
      "objects": {
        "grid": {
          "type": "GeometryCollection",
          "geometries": [
            {"type": "Point", "coordinates": [0, 0], "properties": {"cell": "SW", "value": 10}},
            {"type": "Point", "coordinates": [1, 1], "properties": {"cell": "NE", "value": 20}}
          ]
        }
      },
      "arcs": []
    }"#;

    #[test]
    fn service_hover_matches_shared_hover_helper_for_non_ascii_column() {
        let source =
            "Chart(data: \"data.csv\") {\n  Space(`café` * mass) { Point(fill: `café`) }\n}";
        let (state, uri) = analyzed(source, &[("data.csv", "café,mass\nA,1\n")]);
        let offset = source.find("café").unwrap() + 1;
        let position = offset_to_position(source, offset);

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let service_hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let direct_hover = hover_at(&state, offset);

        let service_hover = service_hover.expect("service hover");
        let direct_hover = direct_hover.expect("direct hover");
        assert_eq!(
            hover_markdown(&service_hover),
            hover_markdown(&direct_hover)
        );
        assert_eq!(service_hover.range, direct_hover.range);
    }

    #[test]
    fn service_hover_previews_primary_source_rows() {
        let source = "Chart(data: \"samples.csv\") {\n  Space(x * y) { Point(fill: group) }\n}";
        let (state, uri) = analyzed(
            source,
            &[("samples.csv", "x,y,group\n1.2,4.0,A\n1.8,4.6,B\n")],
        );
        let offset = source.find("\"samples.csv\"").unwrap() + 2;
        let position = offset_to_position(source, offset);

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("source hover");
        let markdown = hover_markdown(&hover);

        assert!(markdown.contains("Data source `samples.csv`"), "{markdown}");
        assert!(markdown.contains("Sampled schema"), "{markdown}");
        assert!(markdown.contains("| x | float | 1.2, 1.8 |"), "{markdown}");
        assert!(markdown.contains("Sample rows"), "{markdown}");
        assert!(markdown.contains("| 1.2 | 4.0 | A |"), "{markdown}");
        assert!(markdown.contains("Provisional LSP sample"), "{markdown}");
    }

    #[test]
    fn service_schema_resolves_primary_topojson_object() {
        let source = "Chart(data: TopoJson(\"grid.topojson\", object: \"grid\")) {\n  Space(geom, projection: \"equirectangular\") { Geo(fill: value) }\n}";
        let (state, uri) = analyzed(source, &[("grid.topojson", GRID_TOPOJSON)]);
        let offset = source.find("value").unwrap();
        let position = offset_to_position(source, offset);

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("topojson column hover");
        let markdown = hover_markdown(&hover);

        assert!(markdown.contains("Column `value`"), "{markdown}");
        assert!(markdown.contains("Type: `integer`"), "{markdown}");
    }

    #[test]
    fn service_schema_resolves_named_table_topojson_object() {
        let source = "Chart(data: \"main.csv\") {\n  Table grid = TopoJson(\"grid.topojson\", object: \"grid\")\n  Space(geom, data: grid, projection: \"equirectangular\") { Geo(fill: value) }\n}";
        let (state, uri) = analyzed(
            source,
            &[("main.csv", "x\n1\n"), ("grid.topojson", GRID_TOPOJSON)],
        );
        let offset = source.rfind("value").unwrap();
        let position = offset_to_position(source, offset);

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("topojson table column hover");
        let markdown = hover_markdown(&hover);

        assert!(markdown.contains("Column `value`"), "{markdown}");
        assert!(markdown.contains("Source: Table `grid`"), "{markdown}");
        assert!(markdown.contains("Type: `integer`"), "{markdown}");
    }

    #[test]
    fn service_hover_previews_named_table_source_with_non_ascii_text() {
        let source = "Chart(data: \"main.csv\") {\n  Table cities = \"cités.csv\"\n  Space(`café` * pop, data: cities) { Point() }\n}";
        let (state, uri) = analyzed(
            source,
            &[
                ("main.csv", "x,y\n1,2\n"),
                ("cités.csv", "café,pop\nMontréal,20\nQuébec,8\n"),
            ],
        );
        let offset = source.find("cités.csv").unwrap() + "cit".len();
        let position = offset_to_position(source, offset);

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("table source hover");
        let markdown = hover_markdown(&hover);

        assert!(markdown.contains("Data source `cités.csv`"), "{markdown}");
        assert!(markdown.contains("Table: `cities`"), "{markdown}");
        assert!(markdown.contains("café"), "{markdown}");
        assert!(markdown.contains("Montréal"), "{markdown}");
        let range = hover.range.unwrap();
        assert_eq!(
            range.start,
            offset_to_position(source, source.find("\"cités.csv\"").unwrap())
        );
        assert_eq!(range.end.character - range.start.character, 11);
    }

    #[test]
    fn service_hover_handles_unavailable_source_without_echoing_diagnostics() {
        let source = "Chart(data: \"missing.csv\") {\n  Space(x * y) { Point() }\n}";
        let io = TestIo::default();
        let uri = Url::parse("inmemory://algraf/demo.ag").unwrap();
        let cache = InMemorySchemaCache::new();
        let (state, _) = analyze_document_with_io(
            &cache,
            &io,
            &uri,
            0,
            source.to_string(),
            Vec::new(),
            HashMap::new(),
        );
        let position = offset_to_position(source, source.find("missing.csv").unwrap());

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Hover { position });
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("hover");
        let markdown = hover_markdown(&hover);

        assert!(
            markdown.contains("Source preview unavailable"),
            "{markdown}"
        );
        assert!(!markdown.contains("missing missing.csv"), "{markdown}");
    }

    #[test]
    fn service_completion_uses_in_memory_schema() {
        let source = "Chart(data: \"data.csv\") {\n  Space(mass) { Point() }\n}";
        let (state, uri) = analyzed(source, &[("data.csv", "café,mass\nA,1\n")]);
        let position = offset_to_position(source, source.find("Space(").unwrap() + "Space(".len());

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Completion { position });
        let completion: Option<CompletionResponse> =
            serde_json::from_value(response.result).unwrap();
        let items = match completion.expect("completion") {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };

        assert!(items.iter().any(|item| item.label == "café"));
        assert!(items.iter().any(|item| item.label == "mass"));
    }

    #[test]
    fn service_definition_for_source_column_uses_virtual_file_uri() {
        let source = "Chart(data: \"data.csv\") {\n  Space(`café` * mass) { Point() }\n}";
        let (state, uri) = analyzed(source, &[("data.csv", "café,mass\nA,1\n")]);
        let position = offset_to_position(source, source.find("mass").unwrap());

        let response =
            handle_feature_request(&state, &uri, EditorFeatureRequest::Definition { position });
        let definition: Option<GotoDefinitionResponse> =
            serde_json::from_value(response.result).unwrap();
        let GotoDefinitionResponse::Scalar(location) = definition.expect("definition") else {
            panic!("expected scalar definition");
        };

        assert_eq!(location.uri.as_str(), "inmemory://algraf/data.csv");
        assert_eq!(location.range.start.character, 5);
        assert_eq!(location.range.end.character, 9);
    }
}
