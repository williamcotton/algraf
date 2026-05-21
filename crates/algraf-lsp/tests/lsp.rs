use std::path::PathBuf;

use algraf_lsp::Backend;
use futures_util::StreamExt;
use serde_json::json;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    CompletionParams, CompletionResponse, DidOpenTextDocumentParams, Hover, HoverContents,
    HoverParams, InitializeResult, Position, PublishDiagnosticsParams, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, Url,
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

#[test]
fn lsp_position_helper_counts_utf16_code_units() {
    let source = "Chart(data: \"é.csv\") {\n    Space(a * 💧) {}\n}";
    let water = source.find('💧').unwrap();
    let position = utf16_position(source, water);
    assert_eq!(position.line, 1);
    assert_eq!(position.character, 14);
}
