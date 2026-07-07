use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{GlyphClipIr, ScaleIr};

use crate::guide;
use crate::layout::Rect;
use crate::render::backend::RenderScene;
use crate::render::glyph_plan::PlannedGlyph;
use crate::render::panels::{Panel, PlannedLayer};
use crate::sink::MarkSink;
use crate::space::ScaledSpace;

pub(super) fn paint_panel_layers(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for panel in scene.panels {
        paint_panel(sink, scene, panel, diagnostics);
    }
}

fn paint_panel(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    panel: &Panel<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(clip) = panel.clip {
        sink.open_clip(clip.rect);
    }
    paint_layers(
        sink,
        scene,
        &panel.layers,
        panel.table,
        &panel.scaled,
        panel.rows.as_deref(),
        panel.plot,
        &panel.theme,
        &panel.scales,
        diagnostics,
    );
    if panel.clip.is_some() {
        sink.close_clip();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_layers(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    layers: &[PlannedLayer<'_>],
    table: &dyn Table,
    scaled: &ScaledSpace,
    rows: Option<&[usize]>,
    plot: Rect,
    theme: &crate::theme::Theme,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for layer in layers {
        match layer {
            PlannedLayer::Geometry(geo) => crate::geom::render(
                sink,
                geo,
                crate::geom::GeometryRenderContext {
                    space: scaled,
                    table,
                    rows,
                    plot,
                    theme,
                    scales,
                    assets: scene.assets,
                    limits: scene.limits,
                },
                diagnostics,
            ),
            PlannedLayer::Glyph(glyph) => paint_glyph(sink, scene, glyph, diagnostics),
        }
    }
}

fn paint_glyph(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    glyph: &PlannedGlyph<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for instance in &glyph.instances {
        sink.open_layer("algraf-glyph");
        match glyph.clip {
            GlyphClipIr::Rect => sink.open_clip(instance.viewport),
            GlyphClipIr::Circle => {
                sink.open_circle_clip(
                    instance.viewport.x + instance.viewport.width / 2.0,
                    instance.viewport.y + instance.viewport.height / 2.0,
                    instance.viewport.width.min(instance.viewport.height) / 2.0,
                );
            }
            GlyphClipIr::None => {}
        }

        for child_panel in &instance.child_panels {
            paint_child_guides_before(sink, child_panel);
            paint_panel(sink, scene, child_panel, diagnostics);
            paint_child_guides_after(sink, child_panel);
        }

        if !matches!(glyph.clip, GlyphClipIr::None) {
            sink.close_clip();
        }
        sink.close_layer();
    }
}

fn paint_child_guides_before(sink: &mut dyn MarkSink, panel: &Panel<'_>) {
    if !panel.show_guides {
        return;
    }
    super::document::paint_panel_grid(sink, panel);
}

fn paint_child_guides_after(sink: &mut dyn MarkSink, panel: &Panel<'_>) {
    if !panel.show_guides {
        return;
    }
    if panel.scaled.is_polar() {
        guide::render_polar_labels(sink, &panel.scaled, &panel.guides, &panel.theme);
    } else if panel.theme.axes && !panel.scaled.is_spatial() {
        guide::render_axes(
            sink,
            &panel.scaled,
            panel.plot,
            &panel.theme,
            guide::AxisRenderOptions::from_guides(&panel.guides, &panel.theme),
        );
    }
}
