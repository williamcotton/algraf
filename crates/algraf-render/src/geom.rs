//! Geometry rendering (spec §14, §18.6). Supported in version 0.1: Point,
//! Line, Bar (dodge and stack), Rect, and Tile. Other geometries emit a render
//! diagnostic and are skipped.

use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, SettingValue};

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::layout::Rect;
use crate::scale::cell_f64;
use crate::space::ScaledSpace;
use crate::svg::{escape_attr, num, SvgWriter};
use crate::theme::Theme;

const DEFAULT_FILL: &str = "#4E79A7";
const DEFAULT_STROKE: &str = "#333333";

/// Render one geometry layer into the writer.
pub fn render(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    plot: Rect,
    theme: &Theme,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class = format!("algraf-layer algraf-geom-{}", geo_class(geo.kind));
    w.open_group(&format!("class=\"{class}\""));
    match geo.kind {
        GeometryKind::Point => point(w, geo, space, table, theme),
        GeometryKind::Line => line(w, geo, space, table, theme),
        GeometryKind::Bar => bar(w, geo, space, table, plot, diagnostics),
        GeometryKind::Rect => rect(w, geo, space, table),
        GeometryKind::Tile => tile(w, geo, space, table),
        other => diagnostics.push(Diagnostic::warning(
            "R0001",
            format!("geometry `{other:?}` is not yet supported by the renderer"),
            geo.span,
        )),
    }
    w.close_group();
}

fn geo_class(kind: GeometryKind) -> &'static str {
    match kind {
        GeometryKind::Point => "point",
        GeometryKind::Line => "line",
        GeometryKind::Bar => "bar",
        GeometryKind::Rect => "rect",
        GeometryKind::Tile => "tile",
        _ => "other",
    }
}

fn point(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    theme: &Theme,
) {
    let fill = color_spec(geo, "fill", table);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.point_size);
    for row in 0..table.row_count() {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(cx),
            num(cy),
            num(size),
            escape_attr(&color),
            num(alpha),
        ));
    }
}

fn line(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    theme: &Theme,
) {
    let stroke = color_spec(geo, "stroke", table);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);

    // Group rows into series by the stroke category, preserving domain order.
    let groups: Vec<(String, Vec<usize>)> = match &stroke {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                let rows = (0..table.row_count())
                    .filter(|&r| {
                        stroke.resolve(table, r).is_some()
                            && row_category(&stroke, table, r).as_deref() == Some(cat)
                    })
                    .collect();
                (cat.clone(), rows)
            })
            .collect(),
        _ => vec![(String::new(), (0..table.row_count()).collect())],
    };

    for (cat, rows) in groups {
        let mut points: Vec<(f64, f64)> = rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?)))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        if points.is_empty() {
            continue;
        }
        let color = if cat.is_empty() {
            constant_or(&stroke, DEFAULT_STROKE)
        } else {
            stroke
                .resolve(table, *rows.first().unwrap())
                .unwrap_or_else(|| DEFAULT_STROKE.to_string())
        };
        let mut d = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
        }
        w.line(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
            d.trim_end(),
            escape_attr(&color),
            num(width),
            num(alpha),
        ));
    }
}

fn row_category(spec: &ColorSpec, table: &dyn Table, row: usize) -> Option<String> {
    match spec {
        ColorSpec::Categorical { col, .. } => crate::scale::cell_category(table, col, row),
        _ => None,
    }
}

fn bar(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    plot: Rect,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(y_col) = space.y.as_ref().map(|a| a.label()) else {
        return;
    };
    if !space.x.is_band() {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "Bar requires a categorical x dimension",
            geo.span,
        ));
        return;
    }
    let fill = color_spec(geo, "fill", table);
    let alpha = number_setting(geo, "alpha", 1.0);
    let stacked = matches!(
        geo.settings.iter().find(|s| s.name == "layout").map(|s| &s.value),
        Some(SettingValue::String(v)) if v == "stack" || v == "fill"
    );

    let Some(baseline) = space.map_y(0.0) else {
        return;
    };

    if stacked {
        let x_col = space.x.label();
        let mut cumulative: HashMap<String, f64> = HashMap::new();
        for row in 0..table.row_count() {
            let (Some(cx), Some(bw)) = (space.resolve_x(table, row), space.x_bandwidth(table, row))
            else {
                continue;
            };
            let Some(value) = cell_f64(table, &y_col, row) else {
                continue;
            };
            let key = crate::scale::cell_category(table, &x_col, row).unwrap_or_default();
            let base = *cumulative.get(&key).unwrap_or(&0.0);
            let top = base + value;
            cumulative.insert(key, top);
            let (Some(y0), Some(y1)) = (space.map_y(base), space.map_y(top)) else {
                continue;
            };
            emit_bar(w, cx - bw / 2.0, bw, y0, y1, plot, &fill, table, row, alpha);
        }
    } else {
        for row in 0..table.row_count() {
            let (Some(cx), Some(bw)) = (space.resolve_x(table, row), space.x_bandwidth(table, row))
            else {
                continue;
            };
            let Some(top) = space.resolve_y(table, row) else {
                continue;
            };
            emit_bar(
                w,
                cx - bw / 2.0,
                bw,
                baseline,
                top,
                plot,
                &fill,
                table,
                row,
                alpha,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_bar(
    w: &mut SvgWriter,
    x: f64,
    width: f64,
    y_a: f64,
    y_b: f64,
    plot: Rect,
    fill: &ColorSpec,
    table: &dyn Table,
    row: usize,
    alpha: f64,
) {
    let top = y_a.min(y_b).clamp(plot.y, plot.bottom());
    let bottom = y_a.max(y_b).clamp(plot.y, plot.bottom());
    let color = fill
        .resolve(table, row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    w.line(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"{}\" />",
        num(x),
        num(top),
        num(width),
        num(bottom - top),
        escape_attr(&color),
        num(alpha),
    ));
}

fn rect(w: &mut SvgWriter, geo: &GeometryIr, space: &ScaledSpace, table: &dyn Table) {
    let fill = color_spec(geo, "fill", table);
    let alpha = number_setting(geo, "alpha", 1.0);
    for row in 0..table.row_count() {
        let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (
            pos(geo, "xmin", table, row).and_then(|v| space.map_x(v)),
            pos(geo, "xmax", table, row).and_then(|v| space.map_x(v)),
            pos(geo, "ymin", table, row).and_then(|v| space.map_y(v)),
            pos(geo, "ymax", table, row).and_then(|v| space.map_y(v)),
        ) else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(xmin.min(xmax)),
            num(ymin.min(ymax)),
            num((xmax - xmin).abs()),
            num((ymax - ymin).abs()),
            escape_attr(&color),
            num(alpha),
        ));
    }
}

/// The raw value of a positional property: a mapped column cell or a literal.
fn pos(geo: &GeometryIr, name: &str, table: &dyn Table, row: usize) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        return cell_f64(table, &mapping.column.name, row);
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => Some(n),
            _ => None,
        })
}

fn tile(w: &mut SvgWriter, geo: &GeometryIr, space: &ScaledSpace, table: &dyn Table) {
    let fill = color_spec(geo, "fill", table);
    let alpha = number_setting(geo, "alpha", 1.0);
    for row in 0..table.row_count() {
        let (Some(cx), Some(bw), Some(cy), Some(bh)) = (
            space.resolve_x(table, row),
            space.x_bandwidth(table, row),
            space.resolve_y(table, row),
            space.y_bandwidth(table, row),
        ) else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(cx - bw / 2.0),
            num(cy - bh / 2.0),
            num(bw),
            num(bh),
            escape_attr(&color),
            num(alpha),
        ));
    }
}

fn constant_or(spec: &ColorSpec, default: &str) -> String {
    match spec {
        ColorSpec::Constant(c) => c.clone(),
        _ => default.to_string(),
    }
}
