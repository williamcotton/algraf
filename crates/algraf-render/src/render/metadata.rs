//! Stable host-runtime interaction metadata (spec §24.6).
//!
//! The metadata sidecar is built from the planned render scene, after layout and
//! scale training but before any concrete backend serializes output. SVG,
//! draw-list JSON, and host runtimes therefore observe the same plot rectangles,
//! axes, mark positions, tooltip rows, and highlight groups.

use std::fmt::Write as _;

use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, TemporalFormatIr};

use crate::layout::Rect;
use crate::scale::{cell_category, BandScale, ContinuousTransform, NestedBandScale};
use crate::sink::json_string;
use crate::space::AxisScale;
use crate::svg::num;

use super::backend::RenderScene;
use super::panels::{panel_slots, Panel};

/// Versioned interaction metadata emitted as a JSON sidecar and carried by the
/// draw-list backend (spec §24.6).
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionMetadata {
    pub version: u32,
    pub plot_rect: Rect,
    pub axes: InteractionAxes,
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

/// One plot area, used for faceted charts and as the source of the top-level
/// `plot_rect`/`axes` fields for non-faceted charts.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionPlot {
    pub id: String,
    pub plot_rect: Rect,
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

    let mut groups = Vec::new();
    let mut marks = Vec::new();
    for (panel_index, panel) in scene.panels.iter().enumerate() {
        let plot_id = plot_id(panel_index);
        for (geometry_index, geo) in panel.geometries.iter().enumerate() {
            if !supports_mark_metadata(geo.kind) {
                continue;
            }
            collect_geometry_marks(
                &mut marks,
                &mut groups,
                panel,
                geo,
                panel_index,
                geometry_index,
                &plot_id,
            );
        }
    }

    let plots = slots
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
                axes,
            }
        })
        .collect::<Vec<_>>();
    let axes = plots
        .first()
        .map(|plot| plot.axes.clone())
        .unwrap_or_else(|| InteractionAxes { x: None, y: None });

    InteractionMetadata {
        version: 1,
        plot_rect: first_plot,
        axes,
        marks,
        groups,
        plots,
    }
}

fn collect_geometry_marks(
    marks: &mut Vec<InteractionMark>,
    groups: &mut Vec<InteractionGroup>,
    panel: &Panel<'_>,
    geo: &GeometryIr,
    panel_index: usize,
    geometry_index: usize,
    plot_id: &str,
) {
    for row in render_rows(panel.table, panel.rows.as_deref()) {
        let (Some(x_px), Some(y_px)) = (
            panel.scaled.resolve_x(panel.table, row),
            panel.scaled.resolve_y(panel.table, row),
        ) else {
            continue;
        };
        let tooltip = geo
            .interaction
            .tooltip
            .iter()
            .map(|col| TooltipRow {
                label: col.name.clone(),
                value: cell_category(panel.table, &col.name, row).unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let mut mark_groups = Vec::new();
        if let Some(col) = &geo.interaction.highlight {
            if let Some(value) = cell_category(panel.table, &col.name, row) {
                append_group_value(groups, &col.name, &value);
                mark_groups.push(InteractionGroupValue {
                    key: col.name.clone(),
                    value,
                });
            }
        }
        marks.push(InteractionMark {
            id: format!("p{panel_index}:g{geometry_index}:r{row}"),
            plot: plot_id.to_string(),
            x_px,
            y_px,
            groups: mark_groups,
            tooltip,
        });
    }
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
    if panel.scaled.is_spatial() || panel.scaled.is_polar() {
        return InteractionAxes { x: None, y: None };
    }
    InteractionAxes {
        x: Some(axis_metadata(
            &panel.scaled.x,
            panel.guides.x_label.as_deref(),
            panel.guides.x_time_format.as_ref(),
        )),
        y: panel.scaled.y.as_ref().map(|axis| {
            axis_metadata(
                axis,
                panel.guides.y_label.as_deref(),
                panel.guides.y_time_format.as_ref(),
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
        "{{\"id\":{},\"plot\":{},\"x_px\":{},\"y_px\":{},\"groups\":",
        json_string(&mark.id),
        json_string(&mark.plot),
        num(mark.x_px),
        num(mark.y_px),
    );
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
