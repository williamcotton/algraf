//! Geometry rendering (spec §14, §18.6).

mod annotation;
mod bar;
mod common;
mod distribution;
mod geo;
mod line;
mod point;
mod rect_tile;
mod text;

use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, ScaleIr};

use crate::layout::Rect;
use crate::space::ScaledSpace;
use crate::svg::{SvgAttr, SvgWriter};
use crate::theme::Theme;

#[derive(Clone, Copy)]
pub(crate) struct GeometryRenderContext<'a> {
    pub(crate) space: &'a ScaledSpace,
    pub(crate) table: &'a dyn Table,
    pub(crate) rows: Option<&'a [usize]>,
    pub(crate) plot: Rect,
    pub(crate) theme: &'a Theme,
    pub(crate) scales: &'a [ScaleIr],
}

/// Render one geometry layer into the writer.
pub(crate) fn render(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class = format!("algraf-layer algraf-geom-{}", geo.kind.css_class());
    w.open_group_attrs(&[SvgAttr::new("class", class)]);
    let before = w.byte_len();
    match geo.kind {
        GeometryKind::Point => point::render(w, geo, ctx, diagnostics),
        GeometryKind::Line => line::render_polyline(w, geo, ctx, true),
        GeometryKind::Path => line::render_polyline(w, geo, ctx, false),
        GeometryKind::Bar => bar::render(w, geo, ctx, diagnostics),
        GeometryKind::Rect => rect_tile::render_rect(w, geo, ctx),
        GeometryKind::HexBin => distribution::render_hexbin(w, geo, ctx, diagnostics),
        GeometryKind::Tile => rect_tile::render_tile(w, geo, ctx),
        GeometryKind::Smooth => line::render_smooth(w, geo, ctx, diagnostics),
        GeometryKind::Boxplot => distribution::render_boxplot(w, geo, ctx, diagnostics),
        GeometryKind::Violin => distribution::render_violin(w, geo, ctx, diagnostics),
        GeometryKind::Ribbon => line::render_ribbon(w, geo, ctx),
        GeometryKind::HLine => annotation::render_hline(w, geo, ctx),
        GeometryKind::VLine => annotation::render_vline(w, geo, ctx),
        GeometryKind::Rug => annotation::render_rug(w, geo, ctx),
        GeometryKind::Area => line::render_area(w, geo, ctx),
        GeometryKind::Text => text::render(w, geo, ctx),
        GeometryKind::Segment => annotation::render_segment(w, geo, ctx),
        GeometryKind::Geo => geo::render(w, geo, ctx),
        other => diagnostics.push(Diagnostic::warning(
            codes::R0001,
            format!("geometry `{other:?}` is not yet supported by the renderer"),
            geo.span,
        )),
    }
    // W2002: geometry produced no marks (spec §26.3).
    if w.byte_len() == before {
        diagnostics.push(Diagnostic::warning(
            codes::W2002,
            "geometry produced no marks",
            geo.span,
        ));
    }
    w.close_group();
}
