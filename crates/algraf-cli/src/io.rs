//! Output-path helpers and the render-output writer.
//!
//! The render command produces zero or more `RenderOutput` values (one per
//! chart block in the document); this module turns those into bytes on disk
//! or stdout, choosing per-chart paths, PNG rasterization, and sidecar
//! metadata files based on the `RenderArgs`/`RenderFormat` configuration.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cmd_render::{RenderArgs, RenderFormat, RenderOutput, RenderOutputData};
use crate::error::CliError;
use crate::png;

pub(crate) fn write_outputs(
    args: &RenderArgs,
    outputs: Vec<RenderOutput>,
    multi: bool,
) -> Result<(), CliError> {
    for (idx, output) in outputs.into_iter().enumerate() {
        let path = primary_output_path(args.output.as_deref(), idx, multi, args.format);
        write_primary_output(args, path.as_deref(), output.primary)?;
        if let Some(metadata) = output.metadata_json {
            let path = metadata_output_path(
                args.output.as_deref(),
                args.metadata.as_deref(),
                idx,
                multi,
                args.format,
            )?;
            std::fs::write(&path, metadata)
                .map_err(|e| CliError::Io(format!("failed to write {}: {e}", path.display())))?;
        }
    }
    Ok(())
}

pub(crate) fn write_primary_output(
    args: &RenderArgs,
    path: Option<&Path>,
    output: RenderOutputData,
) -> Result<(), CliError> {
    match output {
        // Render-model raster (and any future binary backend) writes bytes.
        RenderOutputData::Bytes(bytes) => match path {
            Some(path) => std::fs::write(path, bytes)
                .map_err(|e| CliError::Io(format!("failed to write {}: {e}", path.display()))),
            None => std::io::stdout()
                .write_all(&bytes)
                .map_err(|e| CliError::Io(format!("failed to write stdout: {e}"))),
        },
        RenderOutputData::Text(text) => match path {
            // The canonical PNG path rasterizes the SVG backend's output.
            Some(path) if args.format.writes_svg() && is_png_path(path) => {
                let png_options =
                    png::PngOptions::new(args.png_scale, args.png_dpi).map_err(CliError::Usage)?;
                png::write_png(text.as_bytes(), path, png_options).map_err(|e| {
                    CliError::Io(format!("failed to write PNG {}: {e}", path.display()))
                })
            }
            Some(path) => std::fs::write(path, text)
                .map_err(|e| CliError::Io(format!("failed to write {}: {e}", path.display()))),
            None => {
                print!("{text}");
                Ok(())
            }
        },
    }
}

pub(crate) fn should_write_metadata(args: &RenderArgs) -> bool {
    args.metadata.is_some() || args.format.writes_metadata()
}

/// Primary output path for chart `idx` (0-based). With a single chart the
/// `--output` path is used verbatim except `--format svg+json`, where an
/// extensionless base gets `.svg`. With multiple charts a 1-based `-{n}` suffix
/// is inserted before the extension (`out.svg` -> `out-1.svg`, `out-2.svg`).
pub(crate) fn primary_output_path(
    base: Option<&Path>,
    idx: usize,
    multi: bool,
    format: RenderFormat,
) -> Option<PathBuf> {
    let path = chart_output_path(base, idx, multi)?;
    if format == RenderFormat::SvgJson && path.extension().is_none() {
        return Some(path.with_extension("svg"));
    }
    Some(path)
}

/// Metadata sidecar path for chart `idx`. An explicit `--metadata` path wins;
/// `--format svg+json` derives `<base>.meta.json` from `--output`.
pub(crate) fn metadata_output_path(
    output: Option<&Path>,
    metadata: Option<&Path>,
    idx: usize,
    multi: bool,
    format: RenderFormat,
) -> Result<PathBuf, CliError> {
    if let Some(path) = metadata {
        return chart_output_path(Some(path), idx, multi).ok_or_else(|| {
            CliError::Usage("internal error: metadata path disappeared".to_string())
        });
    }
    if format == RenderFormat::SvgJson {
        let Some(base) = output else {
            return Err(CliError::Usage(
                "`--format svg+json` requires --output".to_string(),
            ));
        };
        let path = chart_output_path(Some(base), idx, multi).ok_or_else(|| {
            CliError::Usage("internal error: output path disappeared".to_string())
        })?;
        return Ok(path.with_extension("meta.json"));
    }
    Err(CliError::Usage(
        "`--metadata` path is required to write a sidecar".to_string(),
    ))
}

/// Output path for chart `idx` (0-based). With a single chart the base path is
/// used verbatim; with multiple charts a 1-based `-{n}` suffix is inserted
/// before the extension (`out.svg` -> `out-1.svg`, `out-2.svg`).
pub(crate) fn chart_output_path(base: Option<&Path>, idx: usize, multi: bool) -> Option<PathBuf> {
    let base = base?;
    if !multi {
        return Some(base.to_path_buf());
    }
    let n = idx + 1;
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("chart");
    let ext = base.extension().and_then(|s| s.to_str()).unwrap_or("svg");
    Some(base.with_file_name(format!("{stem}-{n}.{ext}")))
}

pub(crate) fn is_png_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}
