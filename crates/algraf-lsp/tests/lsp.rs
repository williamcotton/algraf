use std::path::{Path, PathBuf};

use algraf_lsp::Backend;
use futures_util::StreamExt;
use serde_json::json;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    CodeActionContext, CodeActionParams, CodeActionResponse, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightParams, DocumentRangeFormattingParams, FormattingOptions,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    InitializeResult, InlayHint, InlayHintParams, Location, PartialResultParams, Position,
    PublishDiagnosticsParams, Range, ReferenceContext, ReferenceParams, RenameParams,
    SemanticTokensParams, SemanticTokensResult, SignatureHelp, SignatureHelpParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextEdit, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams, WorkspaceEdit,
};
use tower_lsp::{ClientSocket, LspService};
use tower_service::Service;

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("algraf-lsp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn data_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../algraf-data/tests/fixtures")
        .join(name)
}

async fn initialized_service() -> (tower_lsp::LspService<Backend>, tower_lsp::ClientSocket) {
    let (mut service, socket) = algraf_lsp::build_service();
    let result: InitializeResult =
        request_result(&mut service, "initialize", 1, json!({ "capabilities": {} })).await;

    assert!(result.capabilities.completion_provider.is_some());
    assert!(result.capabilities.hover_provider.is_some());
    assert!(result.capabilities.semantic_tokens_provider.is_some());
    assert!(result.capabilities.code_action_provider.is_some());
    assert!(result.capabilities.inlay_hint_provider.is_none());
    (service, socket)
}

async fn call(service: &mut LspService<Backend>, request: Request) -> Option<Response> {
    std::future::poll_fn(|cx| service.poll_ready(cx))
        .await
        .unwrap();
    service.call(request).await.unwrap()
}

async fn request_result<P, T>(
    service: &mut LspService<Backend>,
    method: &str,
    id: i64,
    params: P,
) -> T
where
    P: serde::Serialize,
    T: serde::de::DeserializeOwned,
{
    let response = call(service, request(method, id, params))
        .await
        .expect("response");
    response_result(response)
}

async fn notify<P>(service: &mut LspService<Backend>, method: &str, params: P)
where
    P: serde::Serialize,
{
    let response = call(
        service,
        Request::build(method.to_string())
            .params(serde_json::to_value(params).unwrap())
            .finish(),
    )
    .await;
    assert!(response.is_none());
}

fn request<P>(method: &str, id: i64, params: P) -> Request
where
    P: serde::Serialize,
{
    Request::build(method.to_string())
        .params(serde_json::to_value(params).unwrap())
        .id(id)
        .finish()
}

fn response_result<T: serde::de::DeserializeOwned>(response: Response) -> T {
    let (_, body) = response.into_parts();
    serde_json::from_value(body.unwrap()).unwrap()
}

async fn open_document(service: &mut LspService<Backend>, uri: Url, text: &str) {
    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri,
            language_id: "algraf".to_string(),
            version: 1,
            text: text.to_string(),
        },
    };
    notify(service, "textDocument/didOpen", params).await;
}

async fn change_document(service: &mut LspService<Backend>, uri: Url, version: i32, text: &str) {
    let params = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri, version },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }],
    };
    notify(service, "textDocument/didChange", params).await;
}

async fn next_client_notification(socket: &mut ClientSocket) -> Request {
    socket.next().await.expect("expected client notification")
}

fn request_position(
    uri: Url,
    text: &str,
    offset: usize,
) -> tower_lsp::lsp_types::TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position: utf16_position(text, offset),
    }
}

fn utf16_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0;
    let mut line_start = 0;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + ch.len_utf8();
        }
    }
    let character = source[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>();
    Position::new(line as u32, character as u32)
}

fn labels(result: Option<CompletionResponse>) -> Vec<(String, Option<String>)> {
    let items = match result.unwrap() {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };
    items
        .into_iter()
        .map(|item| (item.label, item.insert_text))
        .collect()
}

fn hover_markdown(hover: Option<Hover>) -> String {
    match hover.expect("hover").contents {
        HoverContents::Markup(markup) => markup.value,
        other => format!("{other:?}"),
    }
}

fn formatting_options() -> FormattingOptions {
    FormattingOptions {
        tab_size: 4,
        insert_spaces: true,
        ..FormattingOptions::default()
    }
}

#[tokio::test]
async fn completion_quotes_non_identifier_column_names() {
    let dir = temp_project("quoted-columns");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "total revenue,category\n12,A\n").unwrap();

    let source = "Chart(data: \"data.csv\") {\n    Space() {\n        Point()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("Space(").unwrap() + "Space(".len();
    let params = CompletionParams {
        text_document_position: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let result: Option<CompletionResponse> =
        request_result(&mut service, "textDocument/completion", 2, params).await;

    let labels = labels(result);
    assert!(labels
        .iter()
        .any(|(label, insert)| label == "total revenue"
            && insert.as_deref() == Some("`total revenue`")));
}

#[tokio::test]
async fn geometry_property_completion_uses_column_schema() {
    let dir = temp_project("geometry-property");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "species,mass\nAdelie,3700\n").unwrap();

    let source =
        "Chart(data: \"data.csv\") {\n    Space(species * mass) {\n        Point(fill: )\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("fill: ").unwrap() + "fill: ".len();
    let params = CompletionParams {
        text_document_position: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let result: Option<CompletionResponse> =
        request_result(&mut service, "textDocument/completion", 2, params).await;

    let labels = labels(result);
    assert!(labels.iter().any(|(label, _)| label == "species"));
}

#[tokio::test]
async fn scale_type_completion_offers_scale_types() {
    let dir = temp_project("scale-type-completion");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n").unwrap();

    let source =
        "Chart(data: \"data.csv\") {\n    Scale(axis: x, type: )\n    Space(x * y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("type: ").unwrap() + "type: ".len();
    let params = CompletionParams {
        text_document_position: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let result: Option<CompletionResponse> =
        request_result(&mut service, "textDocument/completion", 2, params).await;

    let labels = labels(result);
    assert!(labels.iter().any(|(label, _)| label == "\"sqrt\""));
    assert!(labels.iter().any(|(label, _)| label == "\"log10\""));
    assert!(labels.iter().any(|(label, _)| label == "\"categorical\""));
}

#[tokio::test]
async fn schema_resolution_uses_geojson_constructor_format() {
    let dir = temp_project("geojson-constructor-schema");
    let source_path = dir.join("chart.ag");
    std::fs::copy(data_fixture("tiny.geojson"), dir.join("map.data")).unwrap();

    let source = "Chart(data: GeoJson(\"map.data\")) {\n    Space() { Geo() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("Space(").unwrap() + "Space(".len();
    let params = CompletionParams {
        text_document_position: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let result: Option<CompletionResponse> =
        request_result(&mut service, "textDocument/completion", 2, params).await;

    let labels = labels(result);
    assert!(labels.iter().any(|(label, _)| label == "geom"));
}

#[tokio::test]
async fn schema_resolution_uses_topojson_constructor_object() {
    let dir = temp_project("topojson-constructor-schema");
    let source_path = dir.join("chart.ag");
    std::fs::write(
        dir.join("grid.topojson"),
        r#"{
          "type": "Topology",
          "objects": {
            "grid": {
              "type": "GeometryCollection",
              "geometries": [
                {"type": "Point", "coordinates": [0, 0], "properties": {"value": 10}}
              ]
            }
          },
          "arcs": []
        }"#,
    )
    .unwrap();

    let source = "Chart(data: TopoJson(\"grid.topojson\", object: \"grid\")) {\n    Space(geom, projection: \"equirectangular\") { Geo(fill: value) }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri, source).await;

    let messages = diagnostic_messages(&mut socket).await;
    assert!(messages.is_empty(), "{messages:?}");
}

#[tokio::test]
async fn declaration_completion_knows_scale_and_guide_keys() {
    let dir = temp_project("decl-completion");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "species,mass\nAdelie,3700\n").unwrap();

    let source = "Chart(data: \"data.csv\") {\n    Scale(axis: )\n    Guide(grid: )\n    Space(species * mass) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("axis: ").unwrap() + "axis: ".len();
    let params = CompletionParams {
        text_document_position: request_position(uri.clone(), source, offset),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let result: Option<CompletionResponse> =
        request_result(&mut service, "textDocument/completion", 2, params).await;
    let axis_labels = labels(result);
    assert!(axis_labels.iter().any(|(label, _)| label == "x"));
    assert!(axis_labels.iter().any(|(label, _)| label == "y"));
}

#[tokio::test]
async fn semantic_tokens_full_returns_tokens() {
    let dir = temp_project("semantic-tokens");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n").unwrap();

    let source = "Chart(data: \"data.csv\") {\n    Space(x * y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let params = SemanticTokensParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let result: Option<SemanticTokensResult> =
        request_result(&mut service, "textDocument/semanticTokens/full", 3, params).await;
    let Some(SemanticTokensResult::Tokens(tokens)) = result else {
        panic!("expected semantic tokens");
    };
    assert!(!tokens.data.is_empty());
}

#[tokio::test]
async fn document_formatting_returns_whole_document_edit() {
    let dir = temp_project("formatting");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n").unwrap();

    let source = "Chart(data:\"data.csv\"){Space(x*y){Point()}}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        options: formatting_options(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<Vec<TextEdit>> =
        request_result(&mut service, "textDocument/formatting", 15, params).await;
    let edits = result.expect("formatting edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range.start, Position::new(0, 0));
    assert_eq!(edits[0].range.end, utf16_position(source, source.len()));
    assert_eq!(edits[0].new_text, algraf_syntax::format(source));
}

#[tokio::test]
async fn range_formatting_delegates_to_whole_document_formatting() {
    let dir = temp_project("range-formatting");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n").unwrap();

    let source = "Chart(data:\"data.csv\"){Space(x*y){Point()}}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let params = DocumentRangeFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: utf16_position(source, source.find("Space").unwrap()),
            end: utf16_position(source, source.find("Point").unwrap()),
        },
        options: formatting_options(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<Vec<TextEdit>> =
        request_result(&mut service, "textDocument/rangeFormatting", 16, params).await;
    let edits = result.expect("range formatting edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range.start, Position::new(0, 0));
    assert_eq!(edits[0].range.end, utf16_position(source, source.len()));
    assert_eq!(edits[0].new_text, algraf_syntax::format(source));
}

#[tokio::test]
async fn code_action_quotes_bare_color_literal() {
    let dir = temp_project("code-action-color");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n").unwrap();

    let source = "Chart(data: \"data.csv\") {\n    Space(x * y) { Point(fill: red) }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;
    let notification = next_client_notification(&mut socket).await;
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    let diagnostic = params
        .diagnostics
        .into_iter()
        .find(|diag| {
            diag.code.as_ref().is_some_and(|code| {
                matches!(code, tower_lsp::lsp_types::NumberOrString::String(value) if value == "H3002")
            })
        })
        .expect("expected H3002 diagnostic");

    let action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: diagnostic.range.start,
            end: diagnostic.range.end,
        },
        context: CodeActionContext {
            diagnostics: vec![diagnostic],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let result: Option<CodeActionResponse> =
        request_result(&mut service, "textDocument/codeAction", 4, action_params).await;
    let actions = result.expect("actions");
    let serialized = serde_json::to_string(&actions).unwrap();
    assert!(serialized.contains("\\\"red\\\""), "{serialized}");
}

#[tokio::test]
async fn hover_uses_utf16_positions_for_operator_lookup() {
    let dir = temp_project("utf16-hover");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("é.csv");
    std::fs::write(&data_path, "a,b\n1,2\n").unwrap();

    let source = "Chart(data: \"é.csv\") { Space(a * b) { Point() } }";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find('*').unwrap();
    let params = HoverParams {
        text_document_position_params: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
    };
    let result: Option<Hover> = request_result(&mut service, "textDocument/hover", 2, params).await;

    let value = match result.unwrap().contents {
        HoverContents::Markup(markup) => markup.value,
        other => format!("{other:?}"),
    };
    assert!(value.contains("Cross operator"));
}

#[tokio::test]
async fn hover_previews_source_string_schema_and_rows() {
    let dir = temp_project("source-hover");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("samples.csv"), "x,y,group\n1.2,4.0,A\n1.8,4.6,B\n").unwrap();

    let source = "Chart(data: \"samples.csv\") {\n    Space(x * y) { Point(fill: group) }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("samples.csv").unwrap();
    let params = HoverParams {
        text_document_position_params: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
    };
    let result: Option<Hover> =
        request_result(&mut service, "textDocument/hover", 21, params).await;
    let markdown = hover_markdown(result);

    assert!(markdown.contains("Data source `samples.csv`"), "{markdown}");
    assert!(markdown.contains("| x | float | 1.2, 1.8 |"), "{markdown}");
    assert!(markdown.contains("| 1.2 | 4.0 | A |"), "{markdown}");
}

#[tokio::test]
async fn hover_derived_table_reference_uses_utf16_range() {
    let dir = temp_project("derived-hover");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("données.csv"), "x,y\n1,2\n3,4\n").unwrap();

    let source = "Chart(data: \"données.csv\") {\n    Derive binned = Bin2D(x, y, bins: 10)\n    Space(x_center * y_center, data: binned) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.rfind("binned").unwrap();
    let params = HoverParams {
        text_document_position_params: request_position(uri, source, offset),
        work_done_progress_params: Default::default(),
    };
    let hover: Option<Hover> = request_result(&mut service, "textDocument/hover", 22, params).await;
    let hover = hover.expect("hover");
    let markdown = hover_markdown(Some(hover.clone()));
    assert!(markdown.contains("Derived table `binned`"), "{markdown}");
    assert!(markdown.contains("| x_center | float |"), "{markdown}");

    let range = hover.range.expect("range");
    assert_eq!(range.start, utf16_position(source, offset));
    assert_eq!(
        range.end.character - range.start.character,
        "binned".len() as u32
    );
}

#[tokio::test]
async fn missing_data_file_diagnostic_span_starts_at_string_literal() {
    let dir = temp_project("missing-data-span");
    let source_path = dir.join("chart.ag");

    let source = "Chart(data:\n    \"regional_ales.csv\") {\n    Space(region * sales) {\n        Point()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri, source).await;

    let notification = next_client_notification(&mut socket).await;
    assert_eq!(notification.method(), "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    let diagnostic = params
        .diagnostics
        .iter()
        .find(|diag| {
            diag.code.as_ref().is_some_and(|code| {
            matches!(code, tower_lsp::lsp_types::NumberOrString::String(value) if value == "E1005")
        })
        })
        .expect("expected missing-data diagnostic");

    let start = utf16_position(source, source.find("\"regional_ales.csv\"").unwrap());
    assert_eq!(diagnostic.range.start, start);
}

#[tokio::test]
async fn missing_data_file_keeps_semantic_diagnostics() {
    let dir = temp_project("missing-data-continues-analysis");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("regional_sales.csv");
    std::fs::write(
        &data_path,
        "time,sales,region,product\n2026-01-01,10,North,A\n",
    )
    .unwrap();

    let good_source = "Chrt(data: \"regional_sales.csv\") {\n    Spce((time * sales) / reion) {\n        Line(strke: product)\n    }\n}";
    let bad_source = good_source.replace("regional_sales.csv", "regioal_sales.csv");
    std::fs::write(&source_path, good_source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), good_source).await;
    let _ = next_client_notification(&mut socket).await;

    change_document(&mut service, uri, 2, &bad_source).await;
    let notification = next_client_notification(&mut socket).await;
    assert_eq!(notification.method(), "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();

    let messages = params
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("data file not found")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("unknown column `reion`")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("unknown property `strke`")),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("unknown column `time`")),
        "{messages:?}"
    );
}

#[tokio::test]
async fn caller_input_source_does_not_publish_unknown_columns() {
    for sentinel in ["input", "stdin"] {
        let dir = temp_project(&format!("caller-input-no-unknown-columns-{sentinel}"));
        let source_path = dir.join("chart.ag");
        let source = format!(
            "Chart(data: {sentinel}) {{\n    Space(x * y) {{\n        Point(fill: group, bogus: x)\n    }}\n}}"
        );
        std::fs::write(&source_path, &source).unwrap();
        let uri = Url::from_file_path(&source_path).unwrap();

        let (mut service, mut socket) = initialized_service().await;
        open_document(&mut service, uri, &source).await;

        let notification = next_client_notification(&mut socket).await;
        assert_eq!(notification.method(), "textDocument/publishDiagnostics");
        let params: PublishDiagnosticsParams =
            serde_json::from_value(notification.params().unwrap().clone()).unwrap();

        let messages = params
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("unknown property `bogus`")),
            "{sentinel}: {messages:?}"
        );
        assert!(
            !messages
                .iter()
                .any(|message| message.contains("unknown column")),
            "{sentinel}: {messages:?}"
        );
    }
}

#[tokio::test]
async fn declaration_diagnostic_span_starts_at_declaration_keyword() {
    let dir = temp_project("declaration-span");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    std::fs::write(&data_path, "species,body_mass,flipper_length\nA,1,2\n").unwrap();

    let source = "Chart(data: \"data.csv\", width: 720, height: 480) {\n    Guide(axis: x, label: \"Flipper length (mm)\")\n    Guide(axis: y, label: \"Body mass (g)\")\n    Scale(type: \"log10\")\n\n    Space(flipper_length * body_mass) {\n        Point(fill: species, alpha: 0.7, size: 3)\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri, source).await;

    let notification = next_client_notification(&mut socket).await;
    assert_eq!(notification.method(), "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    let diagnostic = params
        .diagnostics
        .iter()
        .find(|diag| diag.message.contains("`Scale` requires"))
        .expect("expected Scale target diagnostic");

    let scale_start = utf16_position(source, source.find("Scale(").unwrap());
    assert_eq!(diagnostic.range.start, scale_start);
}

// --- v0.4.0 navigation & authoring features ---------------------------------

/// A chart that derives a binned table and uses its output columns.
const BINNED_CHART: &str = "Chart(data: \"data.csv\") {\n    Derive binned = Bin(value, bins: 10)\n    Space(bin_start * count, data: binned) {\n        Rect(xmin: bin_start, xmax: bin_end, ymax: count)\n    }\n}";

async fn open_binned(service: &mut LspService<Backend>, name: &str) -> (Url, String) {
    let dir = temp_project(name);
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "value\n1\n2\n3\n4\n5\n").unwrap();
    std::fs::write(&source_path, BINNED_CHART).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();
    open_document(service, uri.clone(), BINNED_CHART).await;
    (uri, BINNED_CHART.to_string())
}

fn position_params(uri: Url, text: &str, offset: usize) -> TextDocumentPositionParams {
    request_position(uri, text, offset)
}

#[tokio::test]
async fn definition_derived_column_jumps_to_derive() {
    let (mut service, _socket) = initialized_service().await;
    let (uri, source) = open_binned(&mut service, "definition-derived").await;

    // The `bin_start` in the space frame is produced by the `Derive`.
    let offset = source.find("bin_start").unwrap();
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri, &source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let result: Option<GotoDefinitionResponse> =
        request_result(&mut service, "textDocument/definition", 5, params).await;
    let GotoDefinitionResponse::Scalar(location) = result.expect("definition") else {
        panic!("expected scalar definition");
    };
    let expected = utf16_position(&source, source.find("binned").unwrap());
    assert_eq!(location.range.start, expected);
}

#[tokio::test]
async fn definition_data_string_opens_csv_file() {
    let (mut service, _socket) = initialized_service().await;
    let (uri, source) = open_binned(&mut service, "definition-data").await;

    let offset = source.find("data.csv").unwrap();
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri.clone(), &source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let result: Option<GotoDefinitionResponse> =
        request_result(&mut service, "textDocument/definition", 5, params).await;
    let GotoDefinitionResponse::Scalar(location) = result.expect("definition") else {
        panic!("expected scalar definition");
    };
    assert!(location.uri.path().ends_with("data.csv"));
    assert_ne!(location.uri, uri);
}

#[tokio::test]
async fn references_report_column_uses_across_spaces() {
    let dir = temp_project("references-column");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y,z\n1,2,3\n").unwrap();
    let source = "Chart(data: \"data.csv\") {\n    Space(x * y) { Point() }\n    Space(x * z) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("x * y").unwrap();
    let params = ReferenceParams {
        text_document_position: position_params(uri, source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };
    let result: Option<Vec<Location>> =
        request_result(&mut service, "textDocument/references", 6, params).await;
    let locations = result.expect("references");
    assert_eq!(locations.len(), 2, "expected both `x` uses");
}

#[tokio::test]
async fn document_highlight_marks_derive_declaration_and_use() {
    let (mut service, _socket) = initialized_service().await;
    let (uri, source) = open_binned(&mut service, "highlight-derive").await;

    let offset = source.find("data: binned").unwrap() + "data: ".len();
    let params = DocumentHighlightParams {
        text_document_position_params: position_params(uri, &source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let result: Option<Vec<DocumentHighlight>> =
        request_result(&mut service, "textDocument/documentHighlight", 7, params).await;
    let highlights = result.expect("highlights");
    // The declaration name plus the `data: binned` reference.
    assert_eq!(highlights.len(), 2);
    assert!(highlights
        .iter()
        .any(|h| h.kind == Some(tower_lsp::lsp_types::DocumentHighlightKind::WRITE)));
}

#[tokio::test]
async fn references_are_byte_accurate_for_non_ascii_columns() {
    let dir = temp_project("references-utf8");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "naïve,y\n1,2\n").unwrap();
    let source =
        "Chart(data: \"data.csv\") {\n    Space(naïve * y) { Point() }\n    Space(naïve / y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("naïve").unwrap();
    let params = ReferenceParams {
        text_document_position: position_params(uri, source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };
    let locations: Option<Vec<Location>> =
        request_result(&mut service, "textDocument/references", 8, params).await;
    let locations = locations.expect("references");
    assert_eq!(locations.len(), 2);
    let first = utf16_position(source, source.find("naïve").unwrap());
    assert!(locations.iter().any(|loc| loc.range.start == first));
}

#[tokio::test]
async fn signature_help_lists_point_properties() {
    let dir = temp_project("signature-point");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
    let source = "Chart(data: \"data.csv\") {\n    Space(x * y) {\n        Point()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("Point(").unwrap() + "Point(".len();
    let params = SignatureHelpParams {
        context: None,
        text_document_position_params: position_params(uri, source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<SignatureHelp> =
        request_result(&mut service, "textDocument/signatureHelp", 9, params).await;
    let help = result.expect("signature help");
    let signature = &help.signatures[0];
    assert!(signature.label.starts_with("Point("));
    assert!(signature.label.contains("fill"));
    assert_eq!(help.active_parameter, Some(0));
}

#[tokio::test]
async fn signature_help_tracks_active_parameter_past_comma() {
    let dir = temp_project("signature-scale");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
    let source =
        "Chart(data: \"data.csv\") {\n    Scale(axis: x, )\n    Space(x * y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let offset = source.find("axis: x, ").unwrap() + "axis: x, ".len();
    let params = SignatureHelpParams {
        context: None,
        text_document_position_params: position_params(uri, source, offset),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<SignatureHelp> =
        request_result(&mut service, "textDocument/signatureHelp", 10, params).await;
    let help = result.expect("signature help");
    assert!(help.signatures[0].label.starts_with("Scale("));
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn code_action_suggests_corrected_column() {
    let dir = temp_project("code-action-column");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "species,mass\nA,1\n").unwrap();
    let source =
        "Chart(data: \"data.csv\") {\n    Space(species * mass) { Point(fill: speces) }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;
    let notification = next_client_notification(&mut socket).await;
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    let diagnostic = params
        .diagnostics
        .into_iter()
        .find(|diag| {
            diag.code.as_ref().is_some_and(|code| {
                matches!(code, tower_lsp::lsp_types::NumberOrString::String(value) if value == "E1101")
            })
        })
        .expect("expected E1101 diagnostic");

    let action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: diagnostic.range,
        context: CodeActionContext {
            diagnostics: vec![diagnostic],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let result: Option<CodeActionResponse> =
        request_result(&mut service, "textDocument/codeAction", 11, action_params).await;
    let serialized = serde_json::to_string(&result.expect("actions")).unwrap();
    assert!(serialized.contains("Use suggested column"), "{serialized}");
    assert!(serialized.contains("species"), "{serialized}");
}

#[tokio::test]
async fn code_action_desugars_histogram() {
    let dir = temp_project("code-action-histogram");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "value\n1\n2\n3\n").unwrap();
    let source =
        "Chart(data: \"data.csv\") {\n    Space(value) {\n        Histogram(bins: 10)\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let start = source.find("Space(").unwrap();
    let action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: utf16_position(source, start),
            end: utf16_position(source, start),
        },
        context: CodeActionContext {
            diagnostics: vec![],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let result: Option<CodeActionResponse> =
        request_result(&mut service, "textDocument/codeAction", 12, action_params).await;
    let serialized = serde_json::to_string(&result.expect("actions")).unwrap();
    assert!(serialized.contains("Desugar Histogram"), "{serialized}");
    assert!(serialized.contains("Bin(value, bins: 10)"), "{serialized}");
    assert!(serialized.contains("Rect(xmin: bin_start"), "{serialized}");
}

#[tokio::test]
async fn code_action_rewrites_removed_transpose() {
    let dir = temp_project("code-action-transpose");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "quarter,amount\nQ1,10\nQ2,20\n").unwrap();
    let source =
        "Chart(data: \"data.csv\") {\n    Space(transpose((quarter * amount)) / quarter) {\n        Bar()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;
    let notification = next_client_notification(&mut socket).await;
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    let diagnostic = params
        .diagnostics
        .into_iter()
        .find(|diag| {
            diag.code.as_ref().is_some_and(|code| {
                matches!(code, tower_lsp::lsp_types::NumberOrString::String(value) if value == "E1912")
            })
        })
        .expect("expected E1912 diagnostic");

    let action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: diagnostic.range,
        context: CodeActionContext {
            diagnostics: vec![diagnostic],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let result: Option<CodeActionResponse> =
        request_result(&mut service, "textDocument/codeAction", 41, action_params).await;
    let serialized = serde_json::to_string(&result.expect("actions")).unwrap();
    assert!(serialized.contains("Rewrite transpose"), "{serialized}");
    assert!(serialized.contains("(amount * quarter)"), "{serialized}");
}

#[tokio::test]
async fn rename_updates_derived_table_declaration_and_use() {
    let (mut service, _socket) = initialized_service().await;
    let (uri, source) = open_binned(&mut service, "rename-derive").await;

    let offset = source.find("binned").unwrap();
    let params = RenameParams {
        text_document_position: position_params(uri.clone(), &source, offset),
        new_name: "histogram".to_string(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<WorkspaceEdit> =
        request_result(&mut service, "textDocument/rename", 13, params).await;
    let edit = result.expect("rename edit");
    let edits = edit.changes.unwrap().remove(&uri).unwrap();
    assert_eq!(edits.len(), 2, "declaration plus the data: reference");
    assert!(edits.iter().all(|e| e.new_text == "histogram"));
}

#[tokio::test]
async fn rename_updates_let_binding_declaration_and_uses() {
    let dir = temp_project("rename-let");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n3,4\n").unwrap();
    let source = "Chart(data: \"data.csv\") {\n    let primary = \"#3366cc\"\n    Space(x * y) {\n        Point(fill: $primary)\n        Line(stroke: $primary)\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    // Rename from the declaration site.
    let offset = source.find("primary").unwrap();
    let params = RenameParams {
        text_document_position: position_params(uri.clone(), source, offset),
        new_name: "brand".to_string(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<WorkspaceEdit> =
        request_result(&mut service, "textDocument/rename", 40, params).await;
    let edit = result.expect("rename edit");
    let edits = edit.changes.unwrap().remove(&uri).unwrap();
    assert_eq!(edits.len(), 3, "declaration plus two property-value uses");
    assert!(edits.iter().any(|e| e.new_text == "brand"));
    assert_eq!(edits.iter().filter(|e| e.new_text == "$brand").count(), 2);
}

#[tokio::test]
async fn inlay_hints_are_empty_when_legacy_clients_request_them() {
    let (mut service, _socket) = initialized_service().await;
    let (uri, source) = open_binned(&mut service, "inlay-disabled").await;

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: Position::new(0, 0),
            end: utf16_position(&source, source.len()),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let result: Option<Vec<InlayHint>> =
        request_result(&mut service, "textDocument/inlayHint", 14, params).await;
    let hints = result.expect("inlay hints");
    assert!(hints.is_empty());
}

#[tokio::test]
async fn preview_renders_svg_through_render_pipeline() {
    let dir = temp_project("preview-render");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n3,4\n5,6\n").unwrap();
    let source = "Chart(data: \"data.csv\") {\n    Space(x * y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let value: serde_json::Value =
        request_result(&mut service, "algraf/preview", 20, json!({ "uri": uri })).await;
    let svg = value["svg"].as_str().expect("svg string");
    assert!(svg.contains("<svg"), "{svg}");
    let metadata = value["metadata"].as_str().expect("metadata string");
    assert!(metadata.contains("\"version\":1"), "{metadata}");
    assert!(metadata.contains("\"marks\""), "{metadata}");
    assert_eq!(value["superseded"], json!(false));
    // The resolved data dependency is reported so the client can watch it.
    let data_paths = value["dataPaths"].as_array().expect("dataPaths");
    assert_eq!(data_paths.len(), 1);
    assert!(data_paths[0].as_str().unwrap().ends_with("data.csv"));
}

#[tokio::test]
async fn preview_reports_normalized_data_dependency_paths() {
    let dir = temp_project("preview-normalized-dependency");
    let source_dir = dir.join("charts");
    let output_dir = dir.join("outputs");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();
    let source_path = source_dir.join("chart.ag");
    let data_path = output_dir.join("data.csv");
    std::fs::write(&data_path, "x,y\n1,2\n3,4\n").unwrap();
    let source = "Chart(data: \"../outputs/data.csv\") {\n    Space(x * y) { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let value: serde_json::Value =
        request_result(&mut service, "algraf/preview", 24, json!({ "uri": uri })).await;
    assert!(value["svg"].as_str().unwrap().contains("<svg"));
    let data_paths = value["dataPaths"].as_array().expect("dataPaths");
    assert_eq!(data_paths.len(), 1);
    assert_eq!(
        data_paths[0].as_str().unwrap(),
        data_path.display().to_string()
    );
}

#[tokio::test]
async fn preview_preserves_multiline_text_labels() {
    let dir = temp_project("preview-multiline-text");
    let source_path = dir.join("chart.ag");
    std::fs::write(
        dir.join("data.json"),
        r#"[{"x": 1, "y": 2, "label": "first line\nsecond line"}]"#,
    )
    .unwrap();
    let source = "Chart(data: \"data.json\") {\n    Space(x * y) { Text(label: label) }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let value: serde_json::Value =
        request_result(&mut service, "algraf/preview", 23, json!({ "uri": uri })).await;
    let svg = value["svg"].as_str().expect("svg string");
    assert!(
        svg.contains("<tspan") && svg.contains("first line") && svg.contains("second line"),
        "{svg}"
    );
}

#[tokio::test]
async fn preview_loads_named_geojson_table_constructor() {
    let dir = temp_project("preview-named-geojson");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
    std::fs::copy(data_fixture("tiny.geojson"), dir.join("map.data")).unwrap();
    let source = "Chart(data: \"data.csv\") {\n    Table shapes = GeoJson(\"map.data\")\n    Space(geom, data: shapes) { Geo() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let value: serde_json::Value =
        request_result(&mut service, "algraf/preview", 22, json!({ "uri": uri })).await;
    let svg = value["svg"].as_str().expect("svg string");
    assert!(svg.contains("<svg"), "{svg}");
    let data_paths = value["dataPaths"].as_array().expect("dataPaths");
    assert_eq!(data_paths.len(), 2);
    assert!(data_paths
        .iter()
        .any(|path| path.as_str().unwrap().ends_with("map.data")));
}

#[tokio::test]
async fn preview_reports_missing_data_source() {
    let dir = temp_project("preview-missing");
    let source_path = dir.join("chart.ag");
    let source = "Chart() {\n    Space() { Point() }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, _socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;

    let value: serde_json::Value =
        request_result(&mut service, "algraf/preview", 21, json!({ "uri": uri })).await;
    assert!(value["svg"].is_null());
    assert!(value["message"].as_str().unwrap().contains("data source"));
}

async fn diagnostic_messages(socket: &mut ClientSocket) -> Vec<String> {
    let notification = next_client_notification(socket).await;
    assert_eq!(notification.method(), "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notification.params().unwrap().clone()).unwrap();
    params
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

#[tokio::test]
async fn primary_schema_cache_invalidates_when_data_file_changes() {
    let dir = temp_project("primary-cache-invalidation");
    let source_path = dir.join("chart.ag");
    let data_path = dir.join("data.csv");
    // v1 has a `sales` column, so `Space(region * sales)` resolves cleanly.
    std::fs::write(&data_path, "region,sales\nNorth,10\n").unwrap();

    let source =
        "Chart(data: \"data.csv\") {\n    Space(region * sales) {\n        Point()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;
    let before = diagnostic_messages(&mut socket).await;
    assert!(
        !before
            .iter()
            .any(|message| message.contains("unknown column `sales`")),
        "{before:?}"
    );

    // The file changes underneath the editor: `sales` is gone and the byte
    // length differs, so the cached schema must be invalidated and reloaded.
    std::fs::write(&data_path, "region,total_amount\nNorth,10\n").unwrap();
    change_document(&mut service, uri, 2, source).await;
    let after = diagnostic_messages(&mut socket).await;
    assert!(
        after
            .iter()
            .any(|message| message.contains("unknown column `sales`")),
        "{after:?}"
    );
}

#[tokio::test]
async fn named_table_schema_cache_invalidates_when_file_changes() {
    let dir = temp_project("named-table-cache-invalidation");
    let source_path = dir.join("chart.ag");
    std::fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
    let table_path = dir.join("cities.csv");
    // v1 exposes `lat`/`lon`, so the table-bound space resolves cleanly.
    std::fs::write(&table_path, "lat,lon\n1,2\n").unwrap();

    let source = "Chart(data: \"data.csv\") {\n    Table cities = \"cities.csv\"\n    Space(lat * lon, data: cities) {\n        Point()\n    }\n}";
    std::fs::write(&source_path, source).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();

    let (mut service, mut socket) = initialized_service().await;
    open_document(&mut service, uri.clone(), source).await;
    let before = diagnostic_messages(&mut socket).await;
    assert!(
        !before
            .iter()
            .any(|message| message.contains("unknown column `lat`")),
        "{before:?}"
    );

    // The named table's file changes: `lat`/`lon` are renamed and the byte
    // length differs, so the shared cache must reload the table schema too.
    std::fs::write(&table_path, "latitude,longitude\n1,2\n").unwrap();
    change_document(&mut service, uri, 2, source).await;
    let after = diagnostic_messages(&mut socket).await;
    assert!(
        after
            .iter()
            .any(|message| message.contains("unknown column `lat`")),
        "{after:?}"
    );
}

#[test]
fn lsp_position_helper_counts_utf16_code_units() {
    let source = "Chart(data: \"é.csv\") {\n    Space(a * 💧) {}\n}";
    let water = source.find('💧').unwrap();
    let position = utf16_position(source, water);
    assert_eq!(position.line, 1);
    assert_eq!(position.character, 14);
}
