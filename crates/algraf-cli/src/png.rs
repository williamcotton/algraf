//! PNG rasterization for CLI render output.

use std::fs;
use std::path::Path;

use ::png::{BitDepth, ColorType, Encoder, PixelDimensions, Unit};
use resvg::usvg::{Options, Tree};
use tiny_skia::{Pixmap, Transform};

pub const DEFAULT_PNG_SCALE: f32 = 2.0;
const CSS_DPI: f32 = 96.0;

/// PNG-specific rasterization options.
#[derive(Debug, Clone, Copy)]
pub struct PngOptions {
    scale: f32,
    dpi: u32,
}

impl PngOptions {
    pub fn new(scale: f32, dpi: Option<u32>) -> Result<Self, String> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("--png-scale must be a finite number greater than 0".to_string());
        }

        let dpi = match dpi {
            Some(0) => return Err("--png-dpi must be greater than 0".to_string()),
            Some(dpi) => dpi,
            None => (CSS_DPI * scale).round().max(1.0) as u32,
        };

        Ok(PngOptions { scale, dpi })
    }
}

/// Rasterize an SVG document and save it as a PNG file.
pub fn write_png(
    svg_data: &[u8],
    out_path: &Path,
    options: PngOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut opt = Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = Tree::from_data(svg_data, &opt)?;

    let size = tree.size();
    let width = scaled_pixels(size.width(), options.scale)?;
    let height = scaled_pixels(size.height(), options.scale)?;
    let mut pixmap = Pixmap::new(width, height).ok_or("failed to create pixmap")?;

    let transform = Transform::from_scale(options.scale, options.scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    fs::write(out_path, encode_png(&pixmap, options.dpi)?)?;
    Ok(())
}

fn scaled_pixels(value: f32, scale: f32) -> Result<u32, Box<dyn std::error::Error>> {
    let scaled = f64::from(value) * f64::from(scale);
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err("scaled PNG dimensions are too large".into());
    }

    Ok((scaled.round() as u32).max(1))
}

fn encode_png(pixmap: &Pixmap, dpi: u32) -> Result<Vec<u8>, ::png::EncodingError> {
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
        let pixels_per_meter = pixels_per_meter(dpi);
        encoder.set_pixel_dims(Some(PixelDimensions {
            xppu: pixels_per_meter,
            yppu: pixels_per_meter,
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
