use std::collections::HashMap;
use std::path::Path;

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{DataFrame, DataValueRef, Table};
use algraf_driver::{resolve_path, DriverIo, SourceInput};
use algraf_semantics::{
    ChartIr, GeometryIr, GeometryKind, PropertyKey, SettingValue, SpaceIr, SpaceLayerIr,
};

use super::derived::{active_table, compute_derived};

/// One local image asset embedded for `Image(...)` marks.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAsset {
    pub source: String,
    pub href: String,
    pub intrinsic_width: f64,
    pub intrinsic_height: f64,
}

/// Preloaded local image assets keyed by the source string written in `.ag` or
/// held in a mapped `src:` column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageAssets {
    assets: HashMap<String, ImageAsset>,
}

impl ImageAssets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, asset: ImageAsset) {
        self.assets.insert(asset.source.clone(), asset);
    }

    pub fn get(&self, source: &str) -> Option<&ImageAsset> {
        self.assets.get(source)
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

/// Result of collecting and loading image assets before rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAssetLoadResult {
    pub assets: ImageAssets,
    pub diagnostics: Vec<Diagnostic>,
}

/// Collect all literal and mapped `Image(src: ...)` paths reachable from a
/// chart, load them through the host/file I/O boundary, and return an asset
/// store ready for render emission.
pub fn load_image_assets_with_io(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    source_input: &SourceInput,
    base_dir: Option<&Path>,
    io: &dyn DriverIo,
) -> ImageAssetLoadResult {
    let derived = compute_derived(ir, primary, named_tables);
    let mut sources: Vec<(String, Span)> = Vec::new();
    for space in &ir.spaces {
        collect_space_sources(space, primary, &derived, &mut sources);
    }

    let mut assets = ImageAssets::new();
    let mut diagnostics = Vec::new();
    for (source, span) in sources {
        if source.is_empty() || assets.get(&source).is_some() {
            continue;
        }
        match load_one_image(&source, span, source_input, base_dir, io) {
            Ok(asset) => assets.insert(asset),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    ImageAssetLoadResult {
        assets,
        diagnostics,
    }
}

fn collect_space_sources(
    space: &SpaceIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    sources: &mut Vec<(String, Span)>,
) {
    let table = active_table(&space.data, primary, derived);
    for layer in &space.layers {
        match layer {
            SpaceLayerIr::Geometry(geo) => collect_geometry_sources(geo, table, sources),
            SpaceLayerIr::Glyph(glyph) => {
                for child in &glyph.child_spaces {
                    collect_space_sources(child, primary, derived, sources);
                }
            }
        }
    }
}

fn collect_geometry_sources(
    geo: &GeometryIr,
    table: &dyn Table,
    sources: &mut Vec<(String, Span)>,
) {
    if geo.kind != GeometryKind::Image {
        return;
    }
    if let Some(setting) = geo.settings.iter().find(|s| s.name == PropertyKey::Src) {
        if let SettingValue::String(source) = &setting.value {
            push_source(sources, source, setting.span);
        }
    }
    if let Some(mapping) = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Src)
    {
        for row in 0..table.row_count() {
            if let Some(DataValueRef::String(source)) = table.value(&mapping.column.name, row) {
                push_source(sources, source, mapping.span);
            }
        }
    }
}

fn push_source(sources: &mut Vec<(String, Span)>, source: &str, span: Span) {
    if source.is_empty() || sources.iter().any(|(existing, _)| existing == source) {
        return;
    }
    sources.push((source.to_string(), span));
}

fn load_one_image(
    source: &str,
    span: Span,
    source_input: &SourceInput,
    base_dir: Option<&Path>,
    io: &dyn DriverIo,
) -> Result<ImageAsset, Diagnostic> {
    if is_url_like(source) {
        return Err(Diagnostic::error(
            codes::E1204,
            format!("image source `{source}` is a URL; only local image paths are supported"),
            span,
        )
        .with_help("use a chart-relative local path such as \"logos/team.png\""));
    }
    let Some(mime) = mime_for_path(source) else {
        return Err(Diagnostic::error(
            codes::E1204,
            format!("image source `{source}` must end in .png, .jpg, .jpeg, .gif, or .svg"),
            span,
        ));
    };
    let path = resolve_path(source, source_input, base_dir);
    let bytes = io.read_path(&path).map_err(|err| {
        Diagnostic::error(
            codes::E1204,
            format!("failed to load image source `{source}`: {err}"),
            span,
        )
    })?;
    let Some((intrinsic_width, intrinsic_height)) = image_dimensions(mime, &bytes) else {
        return Err(Diagnostic::error(
            codes::E1204,
            format!("image source `{source}` has unsupported or invalid image data"),
            span,
        ));
    };
    Ok(ImageAsset {
        source: source.to_string(),
        href: format!("data:{mime};base64,{}", base64_encode(&bytes)),
        intrinsic_width,
        intrinsic_height,
    })
}

fn mime_for_path(source: &str) -> Option<&'static str> {
    let ext = Path::new(source)
        .extension()
        .and_then(|ext| ext.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn image_dimensions(mime: &str, bytes: &[u8]) -> Option<(f64, f64)> {
    match mime {
        "image/png" => png_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes),
        "image/gif" => gif_dimensions(bytes),
        "image/svg+xml" => svg_dimensions(bytes),
        _ => None,
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    positive_dims(width as f64, height as f64)
}

fn gif_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.len() < 10 || !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return None;
    }
    let width = u16::from_le_bytes(bytes[6..8].try_into().ok()?);
    let height = u16::from_le_bytes(bytes[8..10].try_into().ok()?);
    positive_dims(f64::from(width), f64::from(height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i] == 0xff {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let marker = bytes[i];
        i += 1;
        if matches!(marker, 0xd8 | 0xd9 | 0x01) {
            continue;
        }
        if i + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes(bytes[i..i + 2].try_into().ok()?) as usize;
        if len < 2 || i + len > bytes.len() {
            return None;
        }
        let data = i + 2;
        if is_jpeg_sof(marker) && data + 5 <= bytes.len() {
            let height = u16::from_be_bytes(bytes[data + 1..data + 3].try_into().ok()?);
            let width = u16::from_be_bytes(bytes[data + 3..data + 5].try_into().ok()?);
            return positive_dims(f64::from(width), f64::from(height));
        }
        i += len;
    }
    None
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn svg_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    let text = std::str::from_utf8(bytes).ok()?;
    if let (Some(width), Some(height)) = (
        svg_attr_number(text, "width"),
        svg_attr_number(text, "height"),
    ) {
        if let Some(dims) = positive_dims(width, height) {
            return Some(dims);
        }
    }
    let view_box = svg_attr_value(text, "viewBox")?;
    let nums = view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect::<Vec<_>>();
    if nums.len() == 4 {
        positive_dims(nums[2], nums[3])
    } else {
        None
    }
}

fn svg_attr_number(text: &str, name: &str) -> Option<f64> {
    let raw = svg_attr_value(text, name)?;
    let end = raw
        .char_indices()
        .find_map(|(i, ch)| {
            (!ch.is_ascii_digit() && !matches!(ch, '.' | '-' | '+' | 'e' | 'E')).then_some(i)
        })
        .unwrap_or(raw.len());
    raw[..end].parse::<f64>().ok()
}

fn svg_attr_value<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(name)? + name.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
}

fn positive_dims(width: f64, height: f64) -> Option<(f64, f64)> {
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some((width, height))
}

fn is_url_like(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    if colon == 1 && value.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let scheme = &value[..colon];
    !scheme.is_empty()
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
