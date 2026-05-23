//! Source and data input handling (spec §10.1, §22.3).

use std::io::Read;
use std::path::{Path, PathBuf};

use algraf_data::{
    read_csv, read_path, read_path_as, read_schema_path, read_schema_path_as, ColumnDef, Format,
    LoadResult, Table,
};
use algraf_syntax::ast::{ChartBlock, ChartItem, LiteralKind, Root, ValueExpr};
use algraf_syntax::SyntaxNode;

use crate::error::CliError;

/// Recognize a `GeoJson`/`Shapefile` source constructor and return its explicit
/// format plus the first positional string-literal path (spec §10.11). A plain
/// string path returns `None` so the format is chosen by extension.
fn source_constructor(value: &ValueExpr) -> Option<(Format, String)> {
    let ValueExpr::Call(call) = value else {
        return None;
    };
    let format = match call.name().as_deref() {
        Some("GeoJson") => Format::GeoJson,
        Some("Shapefile") => Format::Shapefile,
        _ => return None,
    };
    let arg = call.args().into_iter().find(|a| a.key().is_none())?;
    match arg.value() {
        Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
            Some((format, strip_string(&lit.text().unwrap_or_default())))
        }
        _ => None,
    }
}

/// Where the Algraf source came from.
pub enum SourceInput {
    Stdin,
    Path(PathBuf),
}

impl SourceInput {
    /// A human-facing label for diagnostics.
    pub fn label(&self) -> String {
        match self {
            SourceInput::Stdin => "<stdin>".to_string(),
            SourceInput::Path(p) => p.display().to_string(),
        }
    }

    pub fn is_stdin(&self) -> bool {
        matches!(self, SourceInput::Stdin)
    }
}

/// The data source declared in the chart's `data` argument.
pub enum AstData {
    /// A path plus an optional explicit format. `None` selects the format by
    /// file extension; a geospatial source constructor sets it explicitly
    /// (spec §10.11).
    Path {
        path: String,
        format: Option<Format>,
    },
    Stdin,
    Missing,
}

enum DataLocation {
    Path {
        path: PathBuf,
        format: Option<Format>,
    },
    Stdin,
}

/// Read Algraf source from a path argument (`-` or absent means stdin).
pub fn read_source(arg: Option<&str>) -> Result<(String, SourceInput), CliError> {
    match arg {
        None | Some("-") => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| CliError::Io(format!("failed to read source from stdin: {e}")))?;
            Ok((text, SourceInput::Stdin))
        }
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| CliError::Io(format!("failed to read {path}: {e}")))?;
            Ok((text, SourceInput::Path(PathBuf::from(path))))
        }
    }
}

/// Extract the first chart's declared data source from the parsed tree
/// (spec §10.1).
pub fn extract_data_source(root: &SyntaxNode) -> AstData {
    let Some(chart) = Root::cast(root.clone()).and_then(|r| r.chart()) else {
        return AstData::Missing;
    };
    extract_chart_data_source(&chart)
}

/// Extract one chart block's declared data source (spec §10.1, §7.1).
pub fn extract_chart_data_source(chart: &ChartBlock) -> AstData {
    for arg in chart.args() {
        if arg.key().as_deref() == Some("data") {
            return match arg.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    AstData::Path {
                        path: strip_string(&lit.text().unwrap_or_default()),
                        format: None,
                    }
                }
                Some(ValueExpr::Stdin(_)) => AstData::Stdin,
                Some(value) => match source_constructor(&value) {
                    Some((format, path)) => AstData::Path {
                        path,
                        format: Some(format),
                    },
                    None => AstData::Missing,
                },
                None => AstData::Missing,
            };
        }
    }
    AstData::Missing
}

/// One chart-scoped named CSV table declaration (`Table name = "path.csv"`).
pub struct NamedTable {
    pub name: String,
    pub frame: algraf_data::DataFrame,
    pub warnings: Vec<algraf_data::DataWarning>,
}

/// Extract `Table name = <source>` declarations from a chart block, carrying
/// each source's optional explicit format (spec §10.x, §10.11).
pub fn extract_chart_tables(chart: &ChartBlock) -> Vec<(String, String, Option<Format>)> {
    let mut out = Vec::new();
    for item in chart.items() {
        let ChartItem::Table(decl) = item else {
            continue;
        };
        let Some(name) = decl.name() else { continue };
        match decl.source() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                out.push((name, strip_string(&lit.text().unwrap_or_default()), None));
            }
            Some(value) => {
                if let Some((format, path)) = source_constructor(&value) {
                    out.push((name, path, Some(format)));
                }
            }
            None => {}
        }
    }
    out
}

/// Load every named `Table` declared in a chart, resolving each path with the
/// same base-dir rules as `Chart(data:)` (spec §10.x). A missing file is
/// `E1106`; an unreadable one is `E1107`.
pub fn load_named_tables(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Result<Vec<NamedTable>, CliError> {
    let base = base_dir
        .map(PathBuf::from)
        .or_else(|| match source {
            SourceInput::Path(p) => p.parent().map(PathBuf::from),
            SourceInput::Stdin => Some(PathBuf::from(".")),
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let mut out = Vec::new();
    for (name, rel, format) in extract_chart_tables(chart) {
        let path = base.join(&rel);
        let loaded = load_path(&path, format).map_err(|e| {
            CliError::Io(format!(
                "failed to load Table `{name}` data {}: {e}",
                path.display()
            ))
        })?;
        out.push(NamedTable {
            name,
            frame: loaded.frame,
            warnings: loaded.warnings,
        });
    }
    Ok(out)
}

/// Load the data table, applying the `--data` override and base-dir resolution
/// (spec §10.1, §22.3).
pub fn load_data(
    ast_data: &AstData,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_opt: Option<&str>,
) -> Result<LoadResult, CliError> {
    match data_location(ast_data, source, base_dir, data_opt)? {
        DataLocation::Path { path, format } => load_path(&path, format)
            .map_err(|e| CliError::Io(format!("failed to load data {}: {e}", path.display()))),
        DataLocation::Stdin => read_stdin_csv(),
    }
}

/// Load a data file, honoring an explicit format when the source named one
/// (a geospatial constructor), else selecting the format by extension.
fn load_path(path: &Path, format: Option<Format>) -> Result<LoadResult, algraf_data::DataError> {
    match format {
        Some(format) => read_path_as(path, format),
        None => read_path(path),
    }
}

/// Load only the resolved data schema, optionally sampling data rows for
/// faster editor/debug workflows (spec §22.6).
pub fn load_schema(
    ast_data: &AstData,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_opt: Option<&str>,
    sample_size: Option<usize>,
) -> Result<Vec<ColumnDef>, CliError> {
    let Some(sample_size) = sample_size else {
        return Ok(load_data(ast_data, source, base_dir, data_opt)?
            .frame
            .schema()
            .to_vec());
    };

    match data_location(ast_data, source, base_dir, data_opt)? {
        DataLocation::Path { path, format } => {
            let result = match format {
                Some(format) => read_schema_path_as(&path, format, sample_size),
                None => read_schema_path(&path, sample_size),
            };
            result.map_err(|e| CliError::Io(format!("failed to load data {}: {e}", path.display())))
        }
        DataLocation::Stdin => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| CliError::Io(format!("failed to read CSV from stdin: {e}")))?;
            algraf_data::read_csv_schema(bytes.as_slice(), sample_size)
                .map_err(|e| CliError::Io(format!("failed to parse stdin CSV: {e}")))
        }
    }
}

fn data_location(
    ast_data: &AstData,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_opt: Option<&str>,
) -> Result<DataLocation, CliError> {
    if let Some(data) = data_opt {
        if data == "-" {
            if source.is_stdin() {
                return Err(CliError::Usage(
                    "cannot read both source and CSV data from stdin".to_string(),
                ));
            }
            return Ok(DataLocation::Stdin);
        }
        // A `--data` override selects its format by extension.
        return Ok(DataLocation::Path {
            path: PathBuf::from(data),
            format: None,
        });
    }

    match ast_data {
        AstData::Stdin => {
            if source.is_stdin() {
                return Err(CliError::Usage(
                    "Chart(data: stdin) but source was also read from stdin; use --data"
                        .to_string(),
                ));
            }
            Ok(DataLocation::Stdin)
        }
        AstData::Path { path: rel, format } => {
            let base = base_dir
                .map(PathBuf::from)
                .or_else(|| match source {
                    SourceInput::Path(p) => p.parent().map(PathBuf::from),
                    SourceInput::Stdin => Some(PathBuf::from(".")),
                })
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(DataLocation::Path {
                path: base.join(rel),
                format: *format,
            })
        }
        AstData::Missing => Err(CliError::Usage(
            "chart has no data source; add Chart(data: \"file.csv\")".to_string(),
        )),
    }
}

fn read_stdin_csv() -> Result<LoadResult, CliError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| CliError::Io(format!("failed to read CSV from stdin: {e}")))?;
    read_csv(bytes.as_slice()).map_err(|e| CliError::Io(format!("failed to parse stdin CSV: {e}")))
}

/// Strip surrounding quotes and resolve escapes in a string literal lexeme.
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
