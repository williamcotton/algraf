use std::path::PathBuf;

use algraf_lsp::Backend;
use futures_util::StreamExt;
use serde_json::json;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    CodeActionContext, CodeActionParams, CodeActionResponse, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Hover, HoverContents, HoverParams,
    InitializeResult, Position, PublishDiagnosticsParams, Range, SemanticTokensParams,
    SemanticTokensResult, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier,
};
use tower_lsp::{ClientSocket, LspService};
use tower_service::Service;

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("algraf-lsp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn initialized_service() -> (tower_lsp::LspService<Backend>, tower_lsp::ClientSocket) {
    let (mut service, socket) = LspService::new(Backend::new);
    let response = call(
        &mut service,
        Request::build("initialize")
            .params(json!({ "capabilities": {} }))
            .id(1)
            .finish(),
    )
    .await
    .unwrap();

    let result: InitializeResult = response_result(response);
    assert!(result.capabilities.completion_provider.is_some());
    assert!(result.capabilities.hover_provider.is_some());
    assert!(result.capabilities.semantic_tokens_provider.is_some());
    assert!(result.capabilities.code_action_provider.is_some());
    (service, socket)
}

async fn call(service: &mut LspService<Backend>, request: Request) -> Option<Response> {
    std::future::poll_fn(|cx| service.poll_ready(cx))
        .await
        .unwrap();
    service.call(request).await.unwrap()
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
    let response = call(
        service,
        Request::build("textDocument/didOpen")
            .params(serde_json::to_value(params).unwrap())
            .finish(),
    )
    .await;
    assert!(response.is_none());
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
    let response = call(
        service,
        Request::build("textDocument/didChange")
            .params(serde_json::to_value(params).unwrap())
            .finish(),
    )
    .await;
    assert!(response.is_none());
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
    let response = call(
        &mut service,
        Request::build("textDocument/completion")
            .params(serde_json::to_value(params).unwrap())
            .id(2)
            .finish(),
    )
    .await
    .unwrap();

    let result: Option<CompletionResponse> = response_result(response);
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
    let response = call(
        &mut service,
        Request::build("textDocument/completion")
            .params(serde_json::to_value(params).unwrap())
            .id(2)
            .finish(),
    )
    .await
    .unwrap();

    let result: Option<CompletionResponse> = response_result(response);
    let labels = labels(result);
    assert!(labels.iter().any(|(label, _)| label == "species"));
}

#[tokio::test]
async fn declaration_completion_knows_v02_scale_and_guide_keys() {
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
    let response = call(
        &mut service,
        Request::build("textDocument/completion")
            .params(serde_json::to_value(params).unwrap())
            .id(2)
            .finish(),
    )
    .await
    .unwrap();
    let axis_labels = labels(response_result(response));
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
    let response = call(
        &mut service,
        Request::build("textDocument/semanticTokens/full")
            .params(serde_json::to_value(params).unwrap())
            .id(3)
            .finish(),
    )
    .await
    .unwrap();
    let result: Option<SemanticTokensResult> = response_result(response);
    let Some(SemanticTokensResult::Tokens(tokens)) = result else {
        panic!("expected semantic tokens");
    };
    assert!(!tokens.data.is_empty());
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
    let response = call(
        &mut service,
        Request::build("textDocument/codeAction")
            .params(serde_json::to_value(action_params).unwrap())
            .id(4)
            .finish(),
    )
    .await
    .unwrap();
    let result: Option<CodeActionResponse> = response_result(response);
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
    let response = call(
        &mut service,
        Request::build("textDocument/hover")
            .params(serde_json::to_value(params).unwrap())
            .id(2)
            .finish(),
    )
    .await
    .unwrap();

    let result: Option<Hover> = response_result(response);
    let value = match result.unwrap().contents {
        HoverContents::Markup(markup) => markup.value,
        other => format!("{other:?}"),
    };
    assert!(value.contains("Cross operator"));
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

#[test]
fn lsp_position_helper_counts_utf16_code_units() {
    let source = "Chart(data: \"é.csv\") {\n    Space(a * 💧) {}\n}";
    let water = source.find('💧').unwrap();
    let position = utf16_position(source, water);
    assert_eq!(position.line, 1);
    assert_eq!(position.character, 14);
}
