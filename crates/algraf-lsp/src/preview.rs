use std::collections::HashMap;

use algraf_core::Severity;
use algraf_driver::{data_dependencies, prepare_chart, PrepareOptions, SourceInput};
use algraf_render::{render_with_tables, Theme};
use algraf_syntax::ast::Root;
use algraf_syntax::{parse, SourceExpr};
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;

use crate::document::source_input_for_uri;
use crate::Backend;

impl Backend {
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

        // Resolve dependency paths in a scope so the `!Send` syntax tree is dropped
        // before the `.await`, keeping the preview future `Send`.
        let data_paths = {
            let syntax = parse(&state.text).syntax();
            let source_input = source_input_for_uri(&uri);
            let Some(chart) = Root::cast(syntax.clone()).and_then(|root| root.chart()) else {
                return Ok(PreviewResult::message(
                    generation,
                    "chart has no data source; add Chart(data: \"file.csv\")",
                ));
            };
            match algraf_syntax::chart_data_source(&chart) {
                SourceExpr::Path { .. } => data_dependencies(&chart, &source_input, None, None)
                    .map(|dependencies| {
                        dependencies
                            .into_iter()
                            .map(|dependency| dependency.path.display().to_string())
                            .collect()
                    })
                    .map_err(|err| err.to_string()),
                SourceExpr::Stdin { .. } => {
                    Err("preview does not support `stdin` data; use a CSV path".to_string())
                }
                SourceExpr::Missing | SourceExpr::Invalid { .. } => {
                    Err("chart has no data source; add Chart(data: \"file.csv\")".to_string())
                }
            }
        };
        let data_paths = match data_paths {
            Ok(paths) => paths,
            Err(message) => return Ok(PreviewResult::message(generation, &message)),
        };

        let text = state.text.clone();
        let source_input = source_input_for_uri(&uri);
        let outcome =
            tokio::task::spawn_blocking(move || render_preview(&text, source_input)).await;

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
fn render_preview(source: &str, source_input: SourceInput) -> Result<String, String> {
    let parsed = parse(source);
    let root = parsed.syntax();
    if parsed
        .diagnostics()
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err("source has parse errors; fix them to preview".to_string());
    }

    let chart = Root::cast(root)
        .and_then(|root| root.chart())
        .ok_or_else(|| "analysis produced no chart".to_string())?;
    let prepared = prepare_chart(
        &chart,
        PrepareOptions {
            source_input: &source_input,
            base_dir: None,
            data_override: None,
            multi_chart: false,
        },
    )
    .map_err(|err| err.to_string())?;
    let analysis = prepared.analysis;
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
    let frame = prepared
        .primary
        .ok_or_else(|| "chart has no data source; add Chart(data: \"file.csv\")".to_string())?
        .frame;
    let named_frames: HashMap<String, algraf_data::DataFrame> = prepared
        .named_tables
        .into_iter()
        .map(|table| (table.name, table.frame))
        .collect();

    let result =
        render_with_tables(&ir, &frame, &named_frames, &theme, None).map_err(|e| e.to_string())?;
    Ok(result.svg)
}
