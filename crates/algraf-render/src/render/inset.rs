use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{InsetClipIr, ScaleIr};

use crate::guide;
use crate::layout::Rect;
use crate::render::backend::RenderScene;
use crate::render::panels::{Panel, PlannedInset, PlannedLayer};
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
    if panel.clip_marks {
        sink.open_clip(panel.plot);
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
    if panel.clip_marks {
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
                    limits: scene.limits,
                },
                diagnostics,
            ),
            PlannedLayer::Inset(inset) => paint_inset(sink, scene, inset, diagnostics),
        }
    }
}

fn paint_inset(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    inset: &PlannedInset<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for instance in &inset.instances {
        sink.open_layer("algraf-inset");
        match inset.clip {
            InsetClipIr::Rect => sink.open_clip(instance.viewport),
            InsetClipIr::Circle => {
                sink.open_circle_clip(
                    instance.viewport.x + instance.viewport.width / 2.0,
                    instance.viewport.y + instance.viewport.height / 2.0,
                    instance.viewport.width.min(instance.viewport.height) / 2.0,
                );
            }
            InsetClipIr::None => {}
        }

        for child_panel in &instance.child_panels {
            paint_child_guides_before(sink, child_panel);
            paint_panel(sink, scene, child_panel, diagnostics);
            paint_child_guides_after(sink, child_panel);
        }

        if !matches!(inset.clip, InsetClipIr::None) {
            sink.close_clip();
        }
        sink.close_layer();
    }
}

fn paint_child_guides_before(sink: &mut dyn MarkSink, panel: &Panel<'_>) {
    if !panel.show_guides {
        return;
    }
    if panel.scaled.is_polar() {
        guide::render_polar_grid(sink, &panel.scaled, &panel.guides, &panel.theme);
    } else if panel.guides.grid && !panel.scaled.is_spatial() {
        guide::render_grid(sink, &panel.scaled, panel.plot, &panel.theme);
    }
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
            guide::AxisRenderOptions {
                x_label_override: panel.guides.x_label.as_deref(),
                y_label_override: panel.guides.y_label.as_deref(),
                x_time_format: panel.guides.x_time_format.as_ref(),
                y_time_format: panel.guides.y_time_format.as_ref(),
                x_tick_label_angle: panel.guides.x_tick_label_angle,
                y_tick_label_angle: panel.guides.y_tick_label_angle,
                x_tick_label_rows: panel.guides.x_tick_label_rows,
                y_tick_label_rows: panel.guides.y_tick_label_rows,
            },
        );
    }
}
