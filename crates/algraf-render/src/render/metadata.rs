//! Stable host-runtime interaction metadata (spec §24.6).
//!
//! The metadata sidecar is built from the planned render scene, after layout and
//! scale training but before any concrete backend serializes output. SVG,
//! draw-list JSON, and host runtimes therefore observe the same plot rectangles,
//! axes, mark positions, tooltip rows, and highlight groups.

use std::fmt::Write as _;

use algraf_data::Table;
use algraf_semantics::{
    GeometryIr, GeometryKind, GuideIr, InsetClipIr, LegendPositionIr, TemporalFormatIr,
};

use crate::layout::Rect;
use crate::scale::{cell_category, BandScale, ContinuousTransform, NestedBandScale};
use crate::sink::json_string;
use crate::space::{AxisScale, ScaledSpace};
use crate::svg::num;

use super::backend::RenderScene;
use super::inset_plan::PlannedInset;
use super::panels::{panel_slots, Panel, PlannedLayer};

/// Versioned interaction metadata emitted as a JSON sidecar and carried by the
/// draw-list backend (spec §24.6).
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionMetadata {
    pub version: u32,
    pub plot_rect: Rect,
    pub axes: InteractionAxes,
    pub chart: InteractionChart,
    pub legend: Option<InteractionLegend>,
    pub marks: Vec<InteractionMark>,
    pub groups: Vec<InteractionGroup>,
    pub plots: Vec<InteractionPlot>,
}

/// Axis metadata for Cartesian host-side inversion.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionAxes {
    pub x: Option<InteractionAxis>,
    pub y: Option<InteractionAxis>,
}

/// Chart-level presentation metadata that hosts can use for accessible labels
/// or non-SVG render targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionChart {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub caption: Option<String>,
    pub alt: Option<String>,
    pub description: Option<String>,
}

/// Legend placement metadata for host renderers.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionLegend {
    pub position: LegendPositionIr,
    pub rect: Rect,
}

/// One plot area, used for faceted charts and as the source of the top-level
/// `plot_rect`/`axes` fields for non-faceted charts.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionPlot {
    pub id: String,
    pub plot_rect: Rect,
    pub clip_rect: Option<Rect>,
    pub axes: InteractionAxes,
}

/// A serializable scale description.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionAxis {
    pub scale: &'static str,
    pub domain: InteractionDomain,
    pub range: [f64; 2],
    pub format: String,
    pub label: String,
    pub padding_inner: Option<f64>,
    pub padding_outer: Option<f64>,
    pub bandwidth: Option<f64>,
    pub inner_domain: Vec<String>,
}

/// Axis domain values in data space. Temporal domains are UTC microseconds since
/// the Unix epoch, matching the renderer's internal temporal scale.
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionDomain {
    Numbers([f64; 2]),
    Integers([i64; 2]),
    Strings(Vec<String>),
}

/// One pickable per-row mark.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionMark {
    pub id: String,
    pub plot: String,
    pub x_px: f64,
    pub y_px: f64,
    pub clipped: bool,
    pub groups: Vec<InteractionGroupValue>,
    pub tooltip: Vec<TooltipRow>,
}

/// A top-level group domain, in first-appearance order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionGroup {
    pub key: String,
    pub values: Vec<String>,
}

/// One mark's group value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionGroupValue {
    pub key: String,
    pub value: String,
}

/// One tooltip row, with display-ready value text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipRow {
    pub label: String,
    pub value: String,
}

impl InteractionMetadata {
    /// Serialize to deterministic JSON with stable key ordering and the same
    /// locale-independent number formatting used by SVG output.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        let _ = write!(out, "\"version\":{},", self.version);
        out.push_str("\"plot_rect\":");
        write_rect_json(&mut out, self.plot_rect);
        out.push_str(",\"axes\":");
        write_axes_json(&mut out, &self.axes);
        out.push_str(",\"chart\":");
        write_chart_json(&mut out, &self.chart);
        out.push_str(",\"legend\":");
        write_legend_json(&mut out, self.legend.as_ref());
        out.push_str(",\"marks\":[");
        for (index, mark) in self.marks.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write_mark_json(&mut out, mark);
        }
        out.push_str("],\"groups\":");
        write_groups_json(&mut out, &self.groups);
        out.push_str(",\"plots\":[");
        for (index, plot) in self.plots.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write_plot_json(&mut out, plot);
        }
        out.push_str("]}");
        out
    }
}

pub(super) fn build_interaction_metadata(scene: &RenderScene<'_>) -> InteractionMetadata {
    let slots = panel_slots(scene.layout, scene.panels);
    let first_plot = slots
        .first()
        .map(|slot| slot.plot)
        .unwrap_or(scene.layout.plot);

    let mut plots = slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let axes = slot
                .panel
                .map(panel_axes)
                .unwrap_or_else(|| InteractionAxes { x: None, y: None });
            InteractionPlot {
                id: plot_id(index),
                plot_rect: slot.plot,
                clip_rect: slot
                    .panel
                    .and_then(|panel| panel.clip_marks.then_some(panel.plot)),
                axes,
            }
        })
        .collect::<Vec<_>>();
    let mut groups = Vec::new();
    let mut marks = Vec::new();
    for (panel_index, panel) in scene.panels.iter().enumerate() {
        let plot_id = plot_id(panel_index);
        collect_layer_metadata(
            &mut marks,
            &mut groups,
            &mut plots,
            &panel.layers,
            panel.table,
            &panel.scaled,
            panel.rows.as_deref(),
            panel.clip_marks.then_some(panel.plot),
            &plot_id,
            &format!("p{panel_index}"),
        );
    }
    let axes = plots
        .first()
        .map(|plot| plot.axes.clone())
        .unwrap_or_else(|| InteractionAxes { x: None, y: None });

    InteractionMetadata {
        version: 1,
        plot_rect: first_plot,
        axes,
        chart: InteractionChart {
            title: scene.ir.title.clone(),
            subtitle: scene.ir.subtitle.clone(),
            caption: scene.ir.caption.clone(),
            alt: scene.ir.alt.clone(),
            description: chart_description(scene.ir),
        },
        legend: scene.layout.legend.map(|rect| InteractionLegend {
            position: scene.theme.legend_position,
            rect,
        }),
        marks,
        groups,
        plots,
    }
}

fn chart_description(ir: &algraf_semantics::ChartIr) -> Option<String> {
    if let Some(description) = &ir.description {
        return Some(description.clone());
    }
    match (&ir.subtitle, &ir.caption) {
        (Some(subtitle), Some(caption)) => Some(format!("{subtitle}\n{caption}")),
        (Some(subtitle), None) => Some(subtitle.clone()),
        (None, Some(caption)) => Some(caption.clone()),
        (None, None) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_layer_metadata(
    marks: &mut Vec<InteractionMark>,
    groups: &mut Vec<InteractionGroup>,
    plots: &mut Vec<InteractionPlot>,
    layers: &[PlannedLayer<'_>],
    table: &dyn Table,
    scaled: &ScaledSpace,
    rows: Option<&[usize]>,
    clip_rect: Option<Rect>,
    plot_id: &str,
    mark_prefix: &str,
) {
    let mut geometry_index = 0;
    let mut inset_index = 0;
    for layer in layers {
        match layer {
            PlannedLayer::Geometry(geo) => {
                if supports_mark_metadata(geo.kind) {
                    collect_geometry_marks(
                        marks,
                        groups,
                        table,
                        scaled,
                        rows,
                        clip_rect,
                        geo,
                        geometry_index,
                        plot_id,
                        mark_prefix,
                    );
                }
                geometry_index += 1;
            }
            PlannedLayer::Inset(inset) => {
                collect_inset_metadata(marks, groups, plots, inset, mark_prefix, inset_index);
                inset_index += 1;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_geometry_marks(
    marks: &mut Vec<InteractionMark>,
    groups: &mut Vec<InteractionGroup>,
    table: &dyn Table,
    scaled: &ScaledSpace,
    rows: Option<&[usize]>,
    clip_rect: Option<Rect>,
    geo: &GeometryIr,
    geometry_index: usize,
    plot_id: &str,
    mark_prefix: &str,
) {
    for row in render_rows(table, rows) {
        let (Some(mut x_px), Some(mut y_px)) =
            (scaled.resolve_x(table, row), scaled.resolve_y(table, row))
        else {
            continue;
        };
        if geo.kind == GeometryKind::Point {
            (x_px, y_px) =
                crate::geom::adjusted_mark_position(geo, scaled, table, row, x_px, y_px, true);
        }
        let tooltip = geo
            .interaction
            .tooltip
            .iter()
            .map(|col| TooltipRow {
                label: col.name.clone(),
                value: cell_category(table, &col.name, row).unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let mut mark_groups = Vec::new();
        if let Some(col) = &geo.interaction.highlight {
            if let Some(value) = cell_category(table, &col.name, row) {
                append_group_value(groups, &col.name, &value);
                mark_groups.push(InteractionGroupValue {
                    key: col.name.clone(),
                    value,
                });
            }
        }
        marks.push(InteractionMark {
            id: format!("{mark_prefix}:g{geometry_index}:r{row}"),
            plot: plot_id.to_string(),
            x_px,
            y_px,
            clipped: clip_rect.is_some_and(|rect| !rect_contains(rect, x_px, y_px)),
            groups: mark_groups,
            tooltip,
        });
    }
}

fn collect_inset_metadata(
    marks: &mut Vec<InteractionMark>,
    groups: &mut Vec<InteractionGroup>,
    plots: &mut Vec<InteractionPlot>,
    inset: &PlannedInset<'_>,
    mark_prefix: &str,
    inset_index: usize,
) {
    for instance in &inset.instances {
        let clip_rect = (!matches!(inset.clip, InsetClipIr::None)).then_some(instance.viewport);
        for (space_index, child_panel) in instance.child_panels.iter().enumerate() {
            let child_plot_id = format!(
                "{mark_prefix}:i{inset_index}[{}]:s{space_index}",
                instance.parent_row
            );
            plots.push(InteractionPlot {
                id: child_plot_id.clone(),
                plot_rect: child_panel.plot,
                clip_rect,
                axes: panel_axes(child_panel),
            });
            collect_layer_metadata(
                marks,
                groups,
                plots,
                &child_panel.layers,
                child_panel.table,
                &child_panel.scaled,
                child_panel.rows.as_deref(),
                clip_rect,
                &child_plot_id,
                &child_plot_id,
            );
        }
    }
}

fn rect_contains(rect: Rect, x: f64, y: f64) -> bool {
    x >= rect.x - f64::EPSILON
        && x <= rect.right() + f64::EPSILON
        && y >= rect.y - f64::EPSILON
        && y <= rect.bottom() + f64::EPSILON
}

fn render_rows(table: &dyn Table, rows: Option<&[usize]>) -> Vec<usize> {
    match rows {
        Some(rows) => rows.to_vec(),
        None => (0..table.row_count()).collect(),
    }
}

fn supports_mark_metadata(kind: GeometryKind) -> bool {
    matches!(
        kind,
        GeometryKind::Point | GeometryKind::Bar | GeometryKind::Rect | GeometryKind::Tile
    )
}

fn append_group_value(groups: &mut Vec<InteractionGroup>, key: &str, value: &str) {
    if let Some(group) = groups.iter_mut().find(|group| group.key == key) {
        if !group.values.iter().any(|existing| existing == value) {
            group.values.push(value.to_string());
        }
        return;
    }
    groups.push(InteractionGroup {
        key: key.to_string(),
        values: vec![value.to_string()],
    });
}

fn panel_axes(panel: &Panel<'_>) -> InteractionAxes {
    scaled_axes(&panel.scaled, &panel.guides)
}

fn scaled_axes(scaled: &ScaledSpace, guides: &GuideIr) -> InteractionAxes {
    if scaled.is_spatial() || scaled.is_polar() {
        return InteractionAxes { x: None, y: None };
    }
    InteractionAxes {
        x: Some(axis_metadata(
            &scaled.x,
            guides.x_label.as_deref(),
            guides.x_time_format.as_ref(),
        )),
        y: scaled.y.as_ref().map(|axis| {
            axis_metadata(
                axis,
                guides.y_label.as_deref(),
                guides.y_time_format.as_ref(),
            )
        }),
    }
}

fn axis_metadata(
    axis: &AxisScale,
    label_override: Option<&str>,
    time_format: Option<&TemporalFormatIr>,
) -> InteractionAxis {
    let label = label_override
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| axis.label());
    match axis {
        AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
            let scale_name = match scale.transform {
                ContinuousTransform::Linear => "linear",
                ContinuousTransform::Log10 => "log10",
                ContinuousTransform::Sqrt => "sqrt",
            };
            InteractionAxis {
                scale: scale_name,
                domain: InteractionDomain::Numbers([scale.min, scale.max]),
                range: [scale.range.0, scale.range.1],
                format: "algraf-number".to_string(),
                label,
                padding_inner: None,
                padding_outer: None,
                bandwidth: None,
                inner_domain: Vec::new(),
            }
        }
        AxisScale::Temporal { scale, .. } | AxisScale::TemporalUnion { scale, .. } => {
            InteractionAxis {
                scale: "time",
                domain: InteractionDomain::Integers([scale.min, scale.max]),
                range: [scale.range.0, scale.range.1],
                format: time_format
                    .map(|format| format.as_str().to_string())
                    .unwrap_or_else(|| "auto".to_string()),
                label,
                padding_inner: None,
                padding_outer: None,
                bandwidth: None,
                inner_domain: Vec::new(),
            }
        }
        AxisScale::Band { scale, .. } => band_axis("band", scale, label, Vec::new()),
        AxisScale::NestedBand { scale, .. } => nested_band_axis(scale, label),
    }
}

fn band_axis(
    scale_name: &'static str,
    scale: &BandScale,
    label: String,
    inner_domain: Vec<String>,
) -> InteractionAxis {
    InteractionAxis {
        scale: scale_name,
        domain: InteractionDomain::Strings(scale.categories.clone()),
        range: [scale.range.0, scale.range.1],
        format: "category".to_string(),
        label,
        padding_inner: Some(scale.pad_inner),
        padding_outer: Some(scale.pad_outer),
        bandwidth: Some(scale.bandwidth()),
        inner_domain,
    }
}

fn nested_band_axis(scale: &NestedBandScale, label: String) -> InteractionAxis {
    InteractionAxis {
        scale: "nested-band",
        domain: InteractionDomain::Strings(scale.outer.categories.clone()),
        range: [scale.outer.range.0, scale.outer.range.1],
        format: "category".to_string(),
        label,
        padding_inner: Some(scale.outer.pad_inner),
        padding_outer: Some(scale.outer.pad_outer),
        bandwidth: Some(scale.outer.bandwidth()),
        inner_domain: scale.inner_categories.clone(),
    }
}

fn plot_id(index: usize) -> String {
    format!("plot{index}")
}

fn write_plot_json(out: &mut String, plot: &InteractionPlot) {
    let _ = write!(out, "{{\"id\":{},\"plot_rect\":", json_string(&plot.id));
    write_rect_json(out, plot.plot_rect);
    if let Some(rect) = plot.clip_rect {
        out.push_str(",\"clip_rect\":");
        write_rect_json(out, rect);
    }
    out.push_str(",\"axes\":");
    write_axes_json(out, &plot.axes);
    out.push('}');
}

fn write_rect_json(out: &mut String, rect: Rect) {
    let _ = write!(
        out,
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        num(rect.x),
        num(rect.y),
        num(rect.width),
        num(rect.height)
    );
}

fn write_axes_json(out: &mut String, axes: &InteractionAxes) {
    out.push('{');
    let mut first = true;
    if let Some(axis) = &axes.x {
        out.push_str("\"x\":");
        write_axis_json(out, axis);
        first = false;
    }
    if let Some(axis) = &axes.y {
        if !first {
            out.push(',');
        }
        out.push_str("\"y\":");
        write_axis_json(out, axis);
    }
    out.push('}');
}

fn write_chart_json(out: &mut String, chart: &InteractionChart) {
    out.push('{');
    write_optional_string(out, "title", chart.title.as_deref(), true);
    write_optional_string(out, "subtitle", chart.subtitle.as_deref(), false);
    write_optional_string(out, "caption", chart.caption.as_deref(), false);
    write_optional_string(out, "alt", chart.alt.as_deref(), false);
    write_optional_string(out, "description", chart.description.as_deref(), false);
    out.push('}');
}

fn write_optional_string(out: &mut String, key: &str, value: Option<&str>, first: bool) {
    if !first {
        out.push(',');
    }
    let _ = write!(out, "{}:", json_string(key));
    match value {
        Some(value) => out.push_str(&json_string(value)),
        None => out.push_str("null"),
    }
}

fn write_legend_json(out: &mut String, legend: Option<&InteractionLegend>) {
    let Some(legend) = legend else {
        out.push_str("null");
        return;
    };
    let _ = write!(
        out,
        "{{\"position\":{},\"rect\":",
        json_string(legend.position.as_str())
    );
    write_rect_json(out, legend.rect);
    out.push('}');
}

fn write_axis_json(out: &mut String, axis: &InteractionAxis) {
    let _ = write!(out, "{{\"scale\":{},\"domain\":", json_string(axis.scale));
    write_domain_json(out, &axis.domain);
    let _ = write!(
        out,
        ",\"range\":[{},{}],\"format\":{},\"label\":{}",
        num(axis.range[0]),
        num(axis.range[1]),
        json_string(&axis.format),
        json_string(&axis.label),
    );
    if let Some(value) = axis.padding_inner {
        let _ = write!(out, ",\"paddingInner\":{}", num(value));
    }
    if let Some(value) = axis.padding_outer {
        let _ = write!(out, ",\"paddingOuter\":{}", num(value));
    }
    if let Some(value) = axis.bandwidth {
        let _ = write!(out, ",\"bandwidth\":{}", num(value));
    }
    if !axis.inner_domain.is_empty() {
        out.push_str(",\"innerDomain\":[");
        for (index, value) in axis.inner_domain.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&json_string(value));
        }
        out.push(']');
    }
    out.push('}');
}

fn write_domain_json(out: &mut String, domain: &InteractionDomain) {
    match domain {
        InteractionDomain::Numbers(values) => {
            let _ = write!(out, "[{},{}]", num(values[0]), num(values[1]));
        }
        InteractionDomain::Integers(values) => {
            let _ = write!(out, "[{},{}]", values[0], values[1]);
        }
        InteractionDomain::Strings(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&json_string(value));
            }
            out.push(']');
        }
    }
}

fn write_mark_json(out: &mut String, mark: &InteractionMark) {
    let _ = write!(
        out,
        "{{\"id\":{},\"plot\":{},\"x_px\":{},\"y_px\":{}",
        json_string(&mark.id),
        json_string(&mark.plot),
        num(mark.x_px),
        num(mark.y_px),
    );
    if mark.clipped {
        out.push_str(",\"clipped\":true");
    }
    out.push_str(",\"groups\":");
    write_group_values_json(out, &mark.groups);
    out.push_str(",\"tooltip\":[");
    for (index, row) in mark.tooltip.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"label\":{},\"value\":{}}}",
            json_string(&row.label),
            json_string(&row.value),
        );
    }
    out.push_str("]}");
}

fn write_group_values_json(out: &mut String, groups: &[InteractionGroupValue]) {
    out.push('{');
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{}:{}",
            json_string(&group.key),
            json_string(&group.value),
        );
    }
    out.push('}');
}

fn write_groups_json(out: &mut String, groups: &[InteractionGroup]) {
    out.push('{');
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}:[", json_string(&group.key));
        for (value_index, value) in group.values.iter().enumerate() {
            if value_index > 0 {
                out.push(',');
            }
            out.push_str(&json_string(value));
        }
        out.push(']');
    }
    out.push('}');
}
