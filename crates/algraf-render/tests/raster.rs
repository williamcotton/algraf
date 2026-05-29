//! Render-model raster backend tests (spec §24.6, §27.1).
//!
//! The raster backend draws from the same planned scene's draw list as the SVG
//! backend. These tests check canvas dimensions, the background, representative
//! mark placement, determinism, and a full-image comparison against the
//! SVG-rasterized baseline within a documented tolerance.

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, render_raster, RasterImage, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;
use resvg::usvg::{Options, Tree};
use tiny_skia::{Pixmap, Transform};

const SCALE: f32 = 2.0;

fn raster(source: &str, csv: &str, theme_override: Option<&str>) -> RasterImage {
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");
    render_raster(&ir, &frame, &Theme::minimal(), theme_override, SCALE)
        .expect("raster")
        .image
}

fn svg_string(source: &str, csv: &str, theme_override: Option<&str>) -> String {
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");
    render(&ir, &frame, &Theme::minimal(), theme_override)
        .expect("svg")
        .svg
}

/// Rasterize an SVG string with resvg — the baseline the render-model raster is
/// compared against.
fn rasterize_svg(svg: &str) -> Pixmap {
    let tree = Tree::from_data(svg.as_bytes(), &Options::default()).expect("usvg tree");
    let size = tree.size();
    let w = (size.width() * SCALE).round() as u32;
    let h = (size.height() * SCALE).round() as u32;
    let mut pixmap = Pixmap::new(w, h).expect("pixmap");
    resvg::render(
        &tree,
        Transform::from_scale(SCALE, SCALE),
        &mut pixmap.as_mut(),
    );
    pixmap
}

#[test]
fn raster_matches_scaled_canvas_dimensions() {
    let img = raster(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
        None,
    );
    // Default canvas is 800x520; the raster grid is scaled by SCALE.
    assert_eq!(img.width(), (800.0 * SCALE) as u32);
    assert_eq!(img.height(), (520.0 * SCALE) as u32);
}

#[test]
fn raster_is_deterministic() {
    let src = "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }";
    let csv = "x,y,g\n1,2,a\n2,3,b\n3,1,a\n";
    let a = raster(src, csv, None);
    let b = raster(src, csv, None);
    assert_eq!(
        a.data(),
        b.data(),
        "identical input yields identical pixels"
    );
}

#[test]
fn raster_background_fills_canvas() {
    // The minimal theme's background is white; the top-left corner is background.
    let img = raster(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
        None,
    );
    let theme = Theme::minimal();
    let (r, g, b) = hex_rgb(&theme.background);
    let px = pixel(&img, 1, 1);
    assert_eq!((px[0], px[1], px[2]), (r, g, b), "corner is the background");
}

#[test]
fn raster_draws_marks() {
    // A void scatter draws only point circles on the background; assert some
    // non-background pixels exist (the marks).
    let img = raster(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n3,1\n4,5\n5,4\n",
        Some("void"),
    );
    let (br, bg, bb) = hex_rgb(&Theme::minimal().background);
    let non_bg = img
        .data()
        .chunks_exact(4)
        .filter(|p| (p[0], p[1], p[2]) != (br, bg, bb))
        .count();
    assert!(non_bg > 0, "marks produce non-background pixels");
}

/// Full-image comparison: for a text-free chart, the render-model raster matches
/// the SVG-rasterized baseline within a documented tolerance. Both rasterize
/// through tiny-skia, so shape rendering agrees closely; remaining differences
/// are sub-pixel anti-aliasing (Design Decision 4).
#[test]
fn raster_matches_svg_baseline_within_tolerance() {
    let src = "Chart(data: \"p.csv\") { Space(x * y) { Point() } }";
    let csv = "x,y\n1,2\n2,3\n3,1\n4,5\n5,4\n";
    let model = raster(src, csv, Some("void"));
    let baseline = rasterize_svg(&svg_string(src, csv, Some("void")));

    assert_eq!(model.width(), baseline.width());
    assert_eq!(model.height(), baseline.height());

    let mean = mean_abs_diff(model.data(), baseline.data());
    assert!(
        mean < 2.0,
        "render-model raster diverged from SVG baseline: mean abs diff {mean}",
    );
}

fn mean_abs_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum();
    sum as f64 / a.len() as f64
}

fn pixel(img: &RasterImage, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * img.width() + x) * 4) as usize;
    let d = img.data();
    [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]]
}

fn hex_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    (
        u8::from_str_radix(&hex[0..2], 16).unwrap(),
        u8::from_str_radix(&hex[2..4], 16).unwrap(),
        u8::from_str_radix(&hex[4..6], 16).unwrap(),
    )
}
