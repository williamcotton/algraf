use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use algraf_core::Severity;
use algraf_driver::{data_dependencies, prepare_chart, OsDriverIo, PrepareOptions, SourceInput};
use algraf_render::{load_image_assets_with_io, render, render_interactive, RenderOptions, Theme};
use algraf_syntax::ast::Root;
use algraf_syntax::{parse, SourceExpr};
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;

use algraf_editor_services::document::source_input_for_uri;

use crate::Backend;

impl Backend {
    /// Render an SVG preview of a document through the same pipeline as
    /// `algraf render` (spec §21.18). Rendering runs on a blocking task so it
    /// never stalls diagnostics, completion, or hover, and a per-document
    /// generation counter discards output that a newer request superseded.
    pub async fn preview(&self, params: PreviewParams) -> LspResult<PreviewResult> {
        let uri = params.uri;
        let interactive = params.interactive;
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
                    "chart has no data source; add Chart(data: \"file.csv\") or Table main = \"file.csv\"",
                ));
            };
            match algraf_syntax::chart_data_source(&chart) {
                SourceExpr::Stdin { .. } => {
                    Err("preview does not support `stdin` data; use a CSV path".to_string())
                }
                SourceExpr::Missing | SourceExpr::Invalid { .. } => {
                    Err(
                        "chart has no data source; add Chart(data: \"file.csv\") or Table main = \"file.csv\""
                            .to_string(),
                    )
                }
                _ => data_dependencies(&chart, &source_input, None, None)
                    .map(|dependencies| {
                        dependencies
                            .into_iter()
                            .map(|dependency| normalize_path(&dependency.path).display().to_string())
                            .collect()
                    })
                    .map_err(|err| err.to_string()),
            }
        };
        let data_paths = match data_paths {
            Ok(paths) => paths,
            Err(message) => return Ok(PreviewResult::message(generation, &message)),
        };

        let text = state.text.clone();
        let source_input = source_input_for_uri(&uri);
        let outcome =
            tokio::task::spawn_blocking(move || render_preview(&text, source_input, interactive))
                .await;

        // If a newer request bumped the counter while we rendered, this output
        // is stale; report supersession rather than returning it (spec §21.13).
        let superseded = preview_superseded(&self.preview_generations, &uri, generation);
        if superseded {
            return Ok(PreviewResult::superseded(generation).with_data_paths(data_paths));
        }

        let result = match outcome {
            Ok(Ok((svg, metadata))) => PreviewResult::svg(generation, svg, metadata),
            Ok(Err(message)) => PreviewResult::message(generation, &message),
            Err(_) => PreviewResult::message(generation, "preview rendering task failed"),
        };
        Ok(result.with_data_paths(data_paths))
    }
}

fn preview_superseded(
    generations: &dashmap::DashMap<Url, u64>,
    uri: &Url,
    generation: u64,
) -> bool {
    generations
        .get(uri)
        .is_some_and(|latest| *latest != generation)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Parameters for the `algraf/preview` custom request.
#[derive(Debug, Clone, Deserialize)]
pub struct PreviewParams {
    /// The document to render.
    pub uri: Url,
    /// Opt into the audited interactive runtime (spec §21.18, §29.3). When
    /// omitted or `false`, the preview SVG is script-free (the default,
    /// script-safe surface). When `true`, the SVG carries only the fixed,
    /// Algraf-shipped runtime for tooltips, highlighting, and plot crosshairs —
    /// never user-authored script.
    #[serde(default)]
    pub interactive: bool,
}

/// Result of the `algraf/preview` custom request.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewResult {
    /// The rendered SVG, when rendering succeeded.
    pub svg: Option<String>,
    /// The JSON interaction sidecar, when rendering succeeded.
    pub metadata: Option<String>,
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
    fn svg(generation: u64, svg: String, metadata: String) -> PreviewResult {
        PreviewResult {
            svg: Some(svg),
            metadata: Some(metadata),
            message: None,
            superseded: false,
            generation,
            data_paths: Vec::new(),
        }
    }

    fn message(generation: u64, message: &str) -> PreviewResult {
        PreviewResult {
            svg: None,
            metadata: None,
            message: Some(message.to_string()),
            superseded: false,
            generation,
            data_paths: Vec::new(),
        }
    }

    fn superseded(generation: u64) -> PreviewResult {
        PreviewResult {
            svg: None,
            metadata: None,
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
fn render_preview(
    source: &str,
    source_input: SourceInput,
    interactive: bool,
) -> Result<(String, String), String> {
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
            data_format_override: None,
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
        .ok_or_else(|| {
            "chart has no data source; add Chart(data: \"file.csv\") or Table main = \"file.csv\""
                .to_string()
        })?
        .frame;
    let named_frames: HashMap<String, algraf_data::DataFrame> = prepared
        .named_tables
        .into_iter()
        .map(|table| (table.name, table.frame))
        .collect();
    let image_assets =
        load_image_assets_with_io(&ir, &frame, &named_frames, &source_input, None, &OsDriverIo);
    if let Some(diagnostic) = image_assets
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostic.message.clone());
    }

    // The preview is script-safe by default; the interactive surface uses only
    // the vetted, non-user runtime (spec §21.18, §29.3).
    let render_options = RenderOptions::default()
        .with_named_tables(&named_frames)
        .with_image_assets(&image_assets.assets);
    let result = if interactive {
        render_interactive(&ir, &frame, &theme, render_options)
    } else {
        render(&ir, &frame, &theme, render_options)
    }
    .map_err(|e| e.to_string())?;
    Ok((result.svg, result.metadata.to_json()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Url;

    fn input() -> SourceInput {
        SourceInput::Path(PathBuf::from("doc.ag"))
    }

    #[test]
    fn parse_errors_block_preview() {
        let err = render_preview("Chart(", input(), false).unwrap_err();
        assert!(err.contains("parse errors"), "got: {err}");
    }

    #[test]
    fn missing_data_file_surfaces_a_preview_error() {
        // A well-formed chart whose data file does not exist must fail to
        // prepare rather than render, and the error is reported (not panicked).
        let source = "Chart(data: \"definitely-missing.csv\") {\n  Space(x * y) { Point() }\n}";
        assert!(render_preview(source, input(), false).is_err());
    }

    #[test]
    fn preview_generation_detects_superseded_results() {
        let generations = dashmap::DashMap::new();
        let uri = Url::parse("file:///chart.ag").unwrap();
        generations.insert(uri.clone(), 2);

        assert!(preview_superseded(&generations, &uri, 1));
        assert!(!preview_superseded(&generations, &uri, 2));
    }
}
