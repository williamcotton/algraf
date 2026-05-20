//! PNG rasterization for CLI render output.

use std::path::Path;

use resvg::usvg::{Options, Tree};
use tiny_skia::Pixmap;

/// Rasterize an SVG document and save it as a PNG file.
pub fn write_png(svg_data: &[u8], out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut opt = Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = Tree::from_data(svg_data, &opt)?;

    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("failed to create pixmap")?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.save_png(out_path)?;
    Ok(())
}
