//! Browser/WASM runtime for Algraf.
//!
//! This crate runs the full parse → analyze → render pipeline in-memory, with
//! no filesystem and no native data backends. It reuses the exact
//! `algraf-driver` → `algraf-render` path the CLI uses (spec §24.6 render
//! execution boundary), so SVG output is byte-identical to the CLI for any
//! chart that does not need an excluded capability (SQLite, shapefile, PNG
//! raster, on-the-fly map projection).
//!
//! Data has no path-backed source in the browser: the host supplies each data
//! source as a name → bytes entry, and [`MemoryIo`] serves those bytes through
//! the existing [`DriverIo`] abstraction, matching on the file name component
//! of the resolved path.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use algraf_driver::InMemorySchemaCache;
use algraf_driver::{
    document_charts, driver_error_diagnostic, extract_chart_data_source, parse_source,
    prepare_chart_with_io, DriverIo, DriverPathMetadata, PrepareOptions, SourceInput,
};
use algraf_editor_services::analysis::analyze_document_with_io;
use algraf_editor_services::document::VirtualFile;
use algraf_editor_services::service::{
    handle_feature_request, EditorFeatureRequest, EditorFeatureResponse,
};
use algraf_render::{render_with_tables, Theme};
use serde::{Deserialize, Serialize};

/// Structured outcome of a render: the SVG (when one was produced) plus every
/// diagnostic gathered along the way, in the same shape the LSP emits.
///
/// `error` carries a catastrophic, span-less failure (an internal renderer
/// error) that has no registered diagnostic code — the same class the CLI maps
/// to an internal error rather than a source diagnostic.
#[derive(Debug, Default)]
pub struct RenderOutcome {
    pub svg: Option<String>,
    pub sidecar: Option<String>,
    pub diagnostics: Vec<algraf_core::Diagnostic>,
    pub error: Option<String>,
}

/// An in-memory [`DriverIo`] backed by a host-supplied `name -> bytes` map.
///
/// The browser has no filesystem; path *resolution* (relative paths, base
/// dirs) still runs in the driver, but the byte fetch is virtual. Lookups match
/// on the final path component so an author's `data: "penguins.csv"` resolves
/// regardless of the synthetic base directory the driver picks.
pub struct MemoryIo {
    files: HashMap<String, Vec<u8>>,
}

impl MemoryIo {
    pub fn new(files: HashMap<String, Vec<u8>>) -> MemoryIo {
        MemoryIo { files }
    }

    fn lookup(&self, path: &Path) -> Option<&Vec<u8>> {
        let name = path.file_name().and_then(|n| n.to_str())?;
        self.files.get(name)
    }
}

impl DriverIo for MemoryIo {
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.lookup(path).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no in-memory data source named {}", path.display()),
            )
        })
    }

    fn read_stdin(&self) -> io::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
        let bytes = self
            .lookup(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))?;
        Ok(DriverPathMetadata {
            len: bytes.len() as u64,
            modified: None,
        })
    }
}

/// Render `.ag` source against host-supplied data, returning SVG + diagnostics.
///
/// `files` maps each data source's file name (e.g. `"penguins.csv"`) to its raw
/// bytes. This is the library entry point a `wasm-bindgen` browser binding
/// wraps; it performs no I/O of its own beyond the supplied map.
pub fn render_to_svg(source: &str, files: HashMap<String, Vec<u8>>) -> RenderOutcome {
    let mut outcome = RenderOutcome::default();
    let io = MemoryIo::new(files);

    let parse = parse_source(source);
    outcome
        .diagnostics
        .extend(parse.diagnostics().iter().cloned());

    let root = parse.syntax();
    let charts = document_charts(&root);
    let Some(chart) = charts.first() else {
        // No chart block: parse diagnostics already explain why.
        return outcome;
    };

    let input = SourceInput::Inline {
        label: "<wasm>".to_string(),
    };
    let prepared = match prepare_chart_with_io(
        chart,
        PrepareOptions {
            source_input: &input,
            base_dir: None,
            data_override: None,
            data_format_override: None,
            multi_chart: false,
        },
        &io,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            let span = extract_chart_data_source(chart)
                .span()
                .unwrap_or_else(|| algraf_core::Span::new(0, 0));
            outcome
                .diagnostics
                .push(driver_error_diagnostic(&err, span));
            return outcome;
        }
    };

    let analysis = prepared.analysis;
    outcome
        .diagnostics
        .extend(analysis.diagnostics.iter().cloned());

    let (Some(ir), Some(loaded)) = (analysis.ir, prepared.primary) else {
        return outcome;
    };

    let named_frames: HashMap<String, algraf_data::DataFrame> = prepared
        .named_tables
        .into_iter()
        .map(|t| (t.name, t.frame))
        .collect();

    let theme = match &ir.theme {
        Some(theme_ir) => Theme::from_ir(theme_ir),
        None => Theme::default(),
    };

    match render_with_tables(&ir, &loaded.frame, &named_frames, &theme, None) {
        Ok(result) => {
            outcome
                .diagnostics
                .extend(result.diagnostics.iter().cloned());
            outcome.sidecar = Some(result.metadata.to_json());
            outcome.svg = Some(result.svg);
        }
        Err(err) => {
            outcome.error = Some(err.to_string());
        }
    }

    outcome
}

#[derive(Debug, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct BrowserRenderRequest {
    source: String,
    files: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct BrowserRenderResponse<'a> {
    svg: Option<&'a str>,
    sidecar: Option<&'a str>,
    diagnostics: &'a [algraf_core::Diagnostic],
    error: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct BrowserEditorRequest {
    source: String,
    #[serde(default)]
    files: HashMap<String, String>,
    #[serde(default = "default_editor_uri")]
    uri: String,
    request: EditorFeatureRequest,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn default_editor_uri() -> String {
    "inmemory://algraf/demo.ag".to_string()
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn render_browser_json(input: &[u8]) -> String {
    let request = match serde_json::from_slice::<BrowserRenderRequest>(input) {
        Ok(request) => request,
        Err(err) => {
            return serde_json::json!({
                "svg": null,
                "sidecar": null,
                "diagnostics": [],
                "error": format!("invalid render request JSON: {err}")
            })
            .to_string();
        }
    };

    let files = request
        .files
        .into_iter()
        .map(|(name, text)| (name, text.into_bytes()))
        .collect();
    let outcome = render_to_svg(&request.source, files);
    let response = BrowserRenderResponse {
        svg: outcome.svg.as_deref(),
        sidecar: outcome.sidecar.as_deref(),
        diagnostics: &outcome.diagnostics,
        error: outcome.error.as_deref(),
    };
    serde_json::to_string(&response).unwrap_or_else(|err| {
        serde_json::json!({
            "svg": null,
            "sidecar": null,
            "diagnostics": [],
            "error": format!("failed to serialize render response: {err}")
        })
        .to_string()
    })
}

pub fn editor_service_response(
    source: String,
    files: HashMap<String, String>,
    uri: lsp_types::Url,
    request: EditorFeatureRequest,
) -> EditorFeatureResponse {
    let io = MemoryIo::new(
        files
            .iter()
            .map(|(name, text)| (name.clone(), text.as_bytes().to_vec()))
            .collect(),
    );
    let virtual_files = files
        .into_iter()
        .map(|(name, text)| {
            (
                name.clone(),
                VirtualFile {
                    uri: virtual_file_uri(&name),
                    text,
                },
            )
        })
        .collect();
    let cache = InMemorySchemaCache::new();
    let (state, _) =
        analyze_document_with_io(&cache, &io, &uri, 0, source, Vec::new(), virtual_files);
    handle_feature_request(&state, &uri, request)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn editor_service_browser_json(input: &[u8]) -> String {
    let request = match serde_json::from_slice::<BrowserEditorRequest>(input) {
        Ok(request) => request,
        Err(err) => {
            return serde_json::to_string(&EditorFeatureResponse::error(format!(
                "invalid editor-service request JSON: {err}"
            )))
            .unwrap_or_else(|_| {
                "{\"diagnostics\":[],\"result\":null,\"error\":\"serialization failed\"}"
                    .to_string()
            });
        }
    };
    let uri = match lsp_types::Url::parse(&request.uri) {
        Ok(uri) => uri,
        Err(err) => {
            return serde_json::to_string(&EditorFeatureResponse::error(format!(
                "invalid editor-service URI: {err}"
            )))
            .unwrap_or_else(|_| {
                "{\"diagnostics\":[],\"result\":null,\"error\":\"serialization failed\"}"
                    .to_string()
            });
        }
    };
    let response = editor_service_response(request.source, request.files, uri, request.request);
    serde_json::to_string(&response).unwrap_or_else(|err| {
        serde_json::json!({
            "diagnostics": [],
            "result": null,
            "error": format!("failed to serialize editor-service response: {err}")
        })
        .to_string()
    })
}

fn virtual_file_uri(name: &str) -> lsp_types::Url {
    let mut uri = lsp_types::Url::parse("inmemory://algraf/").expect("valid in-memory URI");
    uri.set_path(&format!("/{}", name));
    uri
}

#[cfg(target_arch = "wasm32")]
fn pack_ptr_len(ptr: *mut u8, len: usize) -> u64 {
    ((len as u64) << 32) | (ptr as u64)
}

#[cfg(target_arch = "wasm32")]
fn leak_bytes(bytes: Vec<u8>) -> u64 {
    let len = bytes.len();
    let mut bytes = bytes.into_boxed_slice();
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    pack_ptr_len(ptr, len)
}

/// Allocate a byte buffer in WASM linear memory for the browser host.
///
/// The browser writes UTF-8 request JSON into this buffer and then calls
/// [`algraf_render_json`]. Release the buffer with [`algraf_dealloc`].
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn algraf_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// Release a byte buffer allocated by this module.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn algraf_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// Render from browser-supplied request JSON.
///
/// Input shape:
///
/// ```json
/// { "source": "Chart(...)", "files": { "data.json": "[...]" } }
/// ```
///
/// The packed `u64` return value stores the output pointer in the low 32 bits
/// and the output byte length in the high 32 bits. The output is UTF-8 JSON:
/// `{ "svg": string | null, "sidecar": string | null, "diagnostics": [...],
/// "error": string | null }`.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn algraf_render_json(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return leak_bytes(
            serde_json::json!({
                "svg": null,
                "sidecar": null,
                "diagnostics": [],
                "error": "render request pointer was null"
            })
            .to_string()
            .into_bytes(),
        );
    }

    let input = std::slice::from_raw_parts(ptr, len);
    leak_bytes(render_browser_json(input).into_bytes())
}

/// Serve a browser editor-service request through the shared Rust feature
/// helpers used by the native LSP server.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn algraf_editor_service_json(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return leak_bytes(
            serde_json::to_string(&EditorFeatureResponse::error(
                "editor-service request pointer was null",
            ))
            .unwrap_or_else(|_| {
                "{\"diagnostics\":[],\"result\":null,\"error\":\"serialization failed\"}"
                    .to_string()
            })
            .into_bytes(),
        );
    }

    let input = std::slice::from_raw_parts(ptr, len);
    leak_bytes(editor_service_browser_json(input).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_editor_services::positions::offset_to_position;
    use algraf_editor_services::service::EditorFeatureRequest;
    use lsp_types::{CompletionResponse, Hover, HoverContents};

    const CSV: &str = "flipper_length,body_mass,species\n181,3750,Adelie\n186,3800,Adelie\n";
    const SOURCE: &str = "Chart(data: \"penguins.csv\", width: 760, height: 500) {\n    Space(flipper_length * body_mass) { Point(fill: species) }\n}\n";

    fn files() -> HashMap<String, Vec<u8>> {
        let mut files = HashMap::new();
        files.insert("penguins.csv".to_string(), CSV.as_bytes().to_vec());
        files
    }

    #[test]
    fn renders_svg_from_in_memory_data() {
        let outcome = render_to_svg(SOURCE, files());
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let svg = outcome.svg.expect("svg produced");
        assert!(svg.starts_with("<svg "));
    }

    #[test]
    fn missing_data_source_is_a_diagnostic_not_a_panic() {
        let outcome = render_to_svg(SOURCE, HashMap::new());
        assert!(outcome.svg.is_none());
        assert!(
            !outcome.diagnostics.is_empty(),
            "missing data should report a diagnostic"
        );
    }

    #[test]
    fn editor_service_hover_uses_same_in_memory_schema_as_render() {
        let source = "Chart(data: \"penguins.csv\") {\n    Space(flipper_length * body_mass) { Point() }\n}\n";
        let offset = source.find("body_mass").unwrap();
        let response = editor_service_response(
            source.to_string(),
            files()
                .into_iter()
                .map(|(name, bytes)| (name, String::from_utf8(bytes).unwrap()))
                .collect(),
            lsp_types::Url::parse("inmemory://algraf/demo.ag").unwrap(),
            EditorFeatureRequest::Hover {
                position: offset_to_position(source, offset),
            },
        );
        assert!(response.error.is_none(), "{:?}", response.error);
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("hover");
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markdown hover");
        };
        assert!(markup.value.contains("Column `body_mass`"));
        assert!(markup.value.contains("Type: `integer`"));
    }

    #[test]
    fn editor_service_hover_previews_in_memory_source_rows() {
        let source = "Chart(data: \"penguins.csv\") {\n    Space(flipper_length * body_mass) { Point(fill: species) }\n}\n";
        let offset = source.find("penguins.csv").unwrap();
        let response = editor_service_response(
            source.to_string(),
            files()
                .into_iter()
                .map(|(name, bytes)| (name, String::from_utf8(bytes).unwrap()))
                .collect(),
            lsp_types::Url::parse("inmemory://algraf/demo.ag").unwrap(),
            EditorFeatureRequest::Hover {
                position: offset_to_position(source, offset),
            },
        );
        assert!(response.error.is_none(), "{:?}", response.error);
        let hover: Option<Hover> = serde_json::from_value(response.result).unwrap();
        let hover = hover.expect("hover");
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markdown hover");
        };
        assert!(markup.value.contains("Data source `penguins.csv`"));
        assert!(markup.value.contains("Sample rows"));
        assert!(markup.value.contains("| 181 | 3750 | Adelie |"));
    }

    #[test]
    fn editor_service_json_accepts_camel_case_feature_fields() {
        let request = serde_json::json!({
            "source": "Chart(data: \"penguins.csv\") {\n    Space(flipper_length * body_mass) { Point() }\n}\n",
            "files": { "penguins.csv": CSV },
            "uri": "inmemory://algraf/demo.ag",
            "request": {
                "kind": "completion",
                "position": { "line": 1, "character": 10 }
            }
        });
        let response = editor_service_browser_json(request.to_string().as_bytes());
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert!(parsed["error"].is_null(), "{parsed}");
        let completion: Option<CompletionResponse> =
            serde_json::from_value(parsed["result"].clone()).unwrap();
        let labels: Vec<String> = match completion.expect("completion") {
            CompletionResponse::Array(items) => items.into_iter().map(|item| item.label).collect(),
            CompletionResponse::List(list) => {
                list.items.into_iter().map(|item| item.label).collect()
            }
        };
        assert!(labels.contains(&"flipper_length".to_string()));
    }
}
