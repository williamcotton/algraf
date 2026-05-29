//! Geometry rendering (spec §14, §18.6).

mod annotation;
mod bar;
mod common;
mod distribution;
mod geo;
mod graticule;
mod line;
mod point;
mod polar;
mod rect_tile;
mod text;

pub(crate) use common::{DEFAULT_SIZE_RANGE, DEFAULT_STROKE_WIDTH_RANGE};

use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, ScaleIr};

use crate::layout::Rect;
use crate::sink::MarkSink;
use crate::space::ScaledSpace;
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

/// Render one geometry layer into the mark sink (spec §24.6).
pub(crate) fn render(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class = format!("algraf-layer algraf-geom-{}", geo.kind.css_class());
    sink.open_layer(&class);
    let before = sink.primitive_count();
    match geo.kind {
        GeometryKind::Point => point::render(sink, geo, ctx, diagnostics),
        GeometryKind::Line => line::render_polyline(sink, geo, ctx, true),
        GeometryKind::Path => line::render_polyline(sink, geo, ctx, false),
        GeometryKind::Bar => bar::render(sink, geo, ctx, diagnostics),
        GeometryKind::Rect => rect_tile::render_rect(sink, geo, ctx),
        GeometryKind::HexBin => distribution::render_hexbin(sink, geo, ctx, diagnostics),
        GeometryKind::Tile => rect_tile::render_tile(sink, geo, ctx),
        GeometryKind::Smooth => line::render_smooth(sink, geo, ctx, diagnostics),
        GeometryKind::Boxplot => distribution::render_boxplot(sink, geo, ctx, diagnostics),
        GeometryKind::Violin => distribution::render_violin(sink, geo, ctx, diagnostics),
        GeometryKind::Ribbon => line::render_ribbon(sink, geo, ctx),
        GeometryKind::HLine => annotation::render_hline(sink, geo, ctx),
        GeometryKind::VLine => annotation::render_vline(sink, geo, ctx),
        GeometryKind::Rug => annotation::render_rug(sink, geo, ctx),
        GeometryKind::Area => line::render_area(sink, geo, ctx),
        GeometryKind::Text => text::render(sink, geo, ctx),
        GeometryKind::Segment => annotation::render_segment(sink, geo, ctx, diagnostics),
        GeometryKind::Geo => geo::render(sink, geo, ctx),
        GeometryKind::Graticule => graticule::render(sink, geo, ctx),
        other => diagnostics.push(Diagnostic::warning(
            codes::R0001,
            format!("geometry `{other:?}` is not yet supported by the renderer"),
            geo.span,
        )),
    }
    // W2002: geometry produced no marks (spec §26.3).
    if sink.primitive_count() == before {
        diagnostics.push(Diagnostic::warning(
            codes::W2002,
            "geometry produced no marks",
            geo.span,
        ));
    }
    sink.close_layer();
}
