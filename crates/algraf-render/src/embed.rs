//! Embedded rendering facade for host applications (spec §23.2, §29).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use algraf_core::{Diagnostic, Severity, Span};
use algraf_data::{DataFrame, Format};
use algraf_driver::{
    document_charts, driver_error_diagnostic, expand_variables, prepare_chart_with_io,
    DataWarningEntry, DriverError, DriverIo, DriverPathMetadata, LoadContext, PreparationReport,
    PrepareOptions, ReportPhase, SourceInput,
};
use png::{BitDepth, ColorType, Encoder, PixelDimensions, Unit};
use resvg::usvg::{Options as SvgOptions, Tree};
use serde_json::Value;
use tiny_skia::{Pixmap, Transform};

use crate::{
    load_image_assets_with_io, render_interactive_with_tables_and_assets_and_limits,
    render_with_tables_and_assets, Layout, RenderError, RenderLimits, Theme,
};

const DEFAULT_PNG_SCALE: f32 = 2.0;
const CSS_DPI: f32 = 96.0;

/// Output backend exposed by the embedded facade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedOutputFormat {
    Svg,
    Png,
}

impl EmbeddedOutputFormat {
    pub fn content_type(self) -> &'static str {
        match self {
            EmbeddedOutputFormat::Svg => "image/svg+xml",
            EmbeddedOutputFormat::Png => "image/png",
        }
    }
}

/// Host request options for rendering one inline chart.
#[derive(Debug, Clone)]
pub struct EmbeddedRenderOptions {
    pub data_format: Format,
    pub variables: HashMap<String, String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub theme: Option<String>,
    pub output_format: EmbeddedOutputFormat,
    /// Embed Algraf's fixed interactive SVG runtime when rendering SVG output.
    ///
    /// This is ignored for PNG output; PNG continues to rasterize static SVG.
    pub interactive: bool,
    pub strict: bool,
    pub base_dir: Option<PathBuf>,
    pub png_scale: f32,
    pub png_dpi: Option<u32>,
}

impl Default for EmbeddedRenderOptions {
    fn default() -> Self {
        EmbeddedRenderOptions {
            data_format: Format::Csv,
            variables: HashMap::new(),
            width: None,
            height: None,
            theme: None,
            output_format: EmbeddedOutputFormat::Svg,
            interactive: false,
            strict: false,
            base_dir: None,
            png_scale: DEFAULT_PNG_SCALE,
            png_dpi: None,
        }
    }
}

/// Successful embedded render output.
#[derive(Debug, Clone)]
pub struct EmbeddedRenderResult {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub diagnostics: Vec<Diagnostic>,
    pub data_warnings: Vec<DataWarningEntry>,
    pub layout: Layout,
    pub width: u32,
    pub height: u32,
}

impl EmbeddedRenderResult {
    pub fn svg(&self) -> Option<&str> {
        (self.content_type == "image/svg+xml")
            .then(|| std::str::from_utf8(&self.bytes).ok())
            .flatten()
    }
}

/// Errors returned by the embedded facade before or during rendering.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddedRenderError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Driver(String),
    #[error("{0}")]
    Render(String),
    #[error("render blocked by diagnostics")]
    Diagnostics {
        diagnostics: Vec<Diagnostic>,
        data_warnings: Vec<DataWarningEntry>,
    },
}

/// I/O provider for secure embedded defaults: caller input is available, and
/// path reads are denied.
#[derive(Debug, Clone)]
pub struct InputOnlyIo {
    input: Vec<u8>,
}

impl InputOnlyIo {
    pub fn new(input: impl Into<Vec<u8>>) -> Self {
        InputOnlyIo {
            input: input.into(),
        }
    }
}

impl DriverIo for InputOnlyIo {
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
        Err(denied(path))
    }

    fn read_stdin(&self) -> io::Result<Vec<u8>> {
        Ok(self.input.clone())
    }

    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
        Err(denied(path))
    }
}

/// Allowlisted in-memory host I/O provider for embedded tests and middleware.
#[derive(Debug, Clone, Default)]
pub struct InMemoryDriverIo {
    input: Vec<u8>,
    files: HashMap<PathBuf, Vec<u8>>,
}

impl InMemoryDriverIo {
    pub fn new(input: impl Into<Vec<u8>>) -> Self {
        InMemoryDriverIo {
            input: input.into(),
            files: HashMap::new(),
        }
    }

    pub fn with_file(mut self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.insert(path.into(), bytes.into());
        self
    }
}

impl DriverIo for InMemoryDriverIo {
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files.get(path).cloned().ok_or_else(|| denied(path))
    }

    fn read_stdin(&self) -> io::Result<Vec<u8>> {
        Ok(self.input.clone())
    }

    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
        let bytes = self.files.get(path).ok_or_else(|| denied(path))?;
        Ok(DriverPathMetadata {
            len: bytes.len() as u64,
            modified: None,
        })
    }
}

/// Render inline source using secure input-only host I/O.
pub fn render_embedded(
    source: &str,
    input: impl AsRef<[u8]>,
    options: EmbeddedRenderOptions,
) -> Result<EmbeddedRenderResult, EmbeddedRenderError> {
    let io = InputOnlyIo::new(input.as_ref().to_vec());
    render_embedded_with_io(source, &io, options)
}

/// Render inline source from a structured JSON value.
pub fn render_embedded_json(
    source: &str,
    value: &Value,
    mut options: EmbeddedRenderOptions,
) -> Result<EmbeddedRenderResult, EmbeddedRenderError> {
    options.data_format = Format::Json;
    let bytes =
        serde_json::to_vec(value).map_err(|err| EmbeddedRenderError::Usage(err.to_string()))?;
    render_embedded(source, bytes, options)
}

/// Render inline source using host-provided driver I/O.
pub fn render_embedded_with_io(
    source: &str,
    io: &dyn DriverIo,
    options: EmbeddedRenderOptions,
) -> Result<EmbeddedRenderResult, EmbeddedRenderError> {
    let source = expand_variables(source, &options.variables).map_err(map_driver_error)?;
    let parsed = algraf_driver::parse_source(&source);
    let label = "<eval>";
    let source_input = SourceInput::Inline {
        label: label.to_string(),
    };

    let mut report = PreparationReport::new();
    report.extend(ReportPhase::Parse, parsed.diagnostics().iter().cloned());
    if has_blocking(parsed.diagnostics(), options.strict) {
        return Err(EmbeddedRenderError::Diagnostics {
            diagnostics: report.diagnostics(),
            data_warnings: Vec::new(),
        });
    }

    let root = parsed.syntax();
    let charts = document_charts(&root);
    if charts.len() != 1 {
        return Err(EmbeddedRenderError::Usage(format!(
            "embedded rendering expects exactly one Chart block, found {}",
            charts.len()
        )));
    }
    let chart = &charts[0];

    let prepared = match prepare_chart_with_io(
        chart,
        PrepareOptions {
            source_input: &source_input,
            base_dir: options.base_dir.as_deref(),
            data_override: None,
            data_format_override: Some(options.data_format),
            multi_chart: false,
        },
        io,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            let source_expr = algraf_driver::extract_chart_data_source(chart);
            let span = source_expr.span().unwrap_or_else(|| Span::new(0, 0));
            let diagnostic = driver_error_diagnostic(&err, span);
            return Err(EmbeddedRenderError::Diagnostics {
                diagnostics: vec![diagnostic],
                data_warnings: Vec::new(),
            });
        }
    };

    report.extend(
        ReportPhase::Semantic,
        prepared.analysis.diagnostics.iter().cloned(),
    );
    let primary = prepared.primary.ok_or_else(|| {
        EmbeddedRenderError::Driver(
            "analysis allowed rendering with a missing data source".to_string(),
        )
    })?;
    report.push_data_warnings(&LoadContext::Primary, None, &primary.warnings);
    for table in &prepared.named_tables {
        report.push_data_warnings(
            &LoadContext::Table {
                name: table.name.clone(),
            },
            Some(table.path.as_path()),
            &table.warnings,
        );
    }

    let diagnostics = report.diagnostics();
    if has_blocking(&diagnostics, options.strict) || (options.strict && report.has_data_warnings())
    {
        return Err(EmbeddedRenderError::Diagnostics {
            diagnostics,
            data_warnings: report.data_warnings().to_vec(),
        });
    }

    let mut ir = prepared
        .analysis
        .ir
        .ok_or_else(|| EmbeddedRenderError::Driver("analysis produced no IR".to_string()))?;
    if let Some(width) = options.width {
        ir.width = width;
    }
    if let Some(height) = options.height {
        ir.height = height;
    }

    let theme = match (&options.theme, &ir.theme) {
        (Some(name), _) => Theme::by_name(name),
        (None, Some(theme_ir)) => Theme::from_ir(theme_ir),
        (None, None) => Theme::default(),
    };
    let named_frames: HashMap<String, DataFrame> = prepared
        .named_tables
        .into_iter()
        .map(|table| (table.name, table.frame))
        .collect();
    let image_assets = load_image_assets_with_io(
        &ir,
        &primary.frame,
        &named_frames,
        &source_input,
        options.base_dir.as_deref(),
        io,
    );
    report.extend(
        ReportPhase::Render,
        image_assets.diagnostics.iter().cloned(),
    );
    let diagnostics = report.diagnostics();
    if has_blocking(&diagnostics, options.strict) {
        return Err(EmbeddedRenderError::Diagnostics {
            diagnostics,
            data_warnings: report.data_warnings().to_vec(),
        });
    }

    let result = if options.interactive && options.output_format == EmbeddedOutputFormat::Svg {
        render_interactive_with_tables_and_assets_and_limits(
            &ir,
            &primary.frame,
            &named_frames,
            &theme,
            options.theme.as_deref(),
            &image_assets.assets,
            RenderLimits::default(),
        )
    } else {
        render_with_tables_and_assets(
            &ir,
            &primary.frame,
            &named_frames,
            &theme,
            options.theme.as_deref(),
            &image_assets.assets,
        )
    }
    .map_err(map_render_error)?;
    report.extend(ReportPhase::Render, result.diagnostics.iter().cloned());
    let diagnostics = report.diagnostics();
    if has_blocking(&diagnostics, options.strict) {
        return Err(EmbeddedRenderError::Diagnostics {
            diagnostics,
            data_warnings: report.data_warnings().to_vec(),
        });
    }

    let bytes = match options.output_format {
        EmbeddedOutputFormat::Svg => result.svg.into_bytes(),
        EmbeddedOutputFormat::Png => {
            rasterize_png(result.svg.as_bytes(), options.png_scale, options.png_dpi)
                .map_err(EmbeddedRenderError::Render)?
        }
    };

    Ok(EmbeddedRenderResult {
        bytes,
        content_type: options.output_format.content_type(),
        diagnostics,
        data_warnings: report.data_warnings().to_vec(),
        layout: result.layout,
        width: ir.width,
        height: ir.height,
    })
}

fn has_blocking(diagnostics: &[Diagnostic], strict: bool) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            || (strict && diagnostic.severity == Severity::Warning)
    })
}

fn map_driver_error(err: DriverError) -> EmbeddedRenderError {
    match err {
        DriverError::Usage(message) => EmbeddedRenderError::Usage(message),
        DriverError::Data { .. } | DriverError::StdinRead(_) | DriverError::StdinParse(_) => {
            EmbeddedRenderError::Driver(err.to_string())
        }
    }
}

fn map_render_error(err: RenderError) -> EmbeddedRenderError {
    EmbeddedRenderError::Render(err.to_string())
}

fn denied(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("host I/O denied for {}", path.display()),
    )
}

fn rasterize_png(svg_data: &[u8], scale: f32, dpi: Option<u32>) -> Result<Vec<u8>, String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err("png_scale must be a finite number greater than 0".to_string());
    }
    let dpi = match dpi {
        Some(0) => return Err("png_dpi must be greater than 0".to_string()),
        Some(dpi) => dpi,
        None => (CSS_DPI * scale).round().max(1.0) as u32,
    };

    let mut opt = SvgOptions::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = Tree::from_data(svg_data, &opt).map_err(|err| err.to_string())?;
    let size = tree.size();
    let width = scaled_pixels(size.width(), scale)?;
    let height = scaled_pixels(size.height(), scale)?;
    let mut pixmap = Pixmap::new(width, height).ok_or("failed to create pixmap")?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    encode_png(&pixmap, dpi).map_err(|err| err.to_string())
}

fn scaled_pixels(value: f32, scale: f32) -> Result<u32, String> {
    let scaled = f64::from(value) * f64::from(scale);
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err("scaled PNG dimensions are too large".to_string());
    }
    Ok((scaled.round() as u32).max(1))
}

fn encode_png(pixmap: &Pixmap, dpi: u32) -> Result<Vec<u8>, png::EncodingError> {
    let pixmap_ref = pixmap.as_ref();
    let mut rgba = Vec::with_capacity(pixmap_ref.data().len());
    for pixel in pixmap_ref.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }

    let mut data = Vec::new();
    {
        let mut encoder = Encoder::new(&mut data, pixmap_ref.width(), pixmap_ref.height());
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_pixel_dims(Some(PixelDimensions {
            xppu: pixels_per_meter(dpi),
            yppu: pixels_per_meter(dpi),
            unit: Unit::Meter,
        }));
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
    }
    Ok(data)
}

fn pixels_per_meter(dpi: u32) -> u32 {
    (f64::from(dpi) / 0.0254).round() as u32
}
