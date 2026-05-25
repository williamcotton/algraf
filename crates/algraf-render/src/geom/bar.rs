use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::GeometryIr;

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::helpers::{bar_layout, BarLayout};
use crate::layout::Rect;
use crate::scale::{cell_category, cell_f64};
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{render_rows, stroke_attrs, DEFAULT_FILL};
use super::GeometryRenderContext;

pub(super) fn render(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let plot = ctx.plot;
    let scales = ctx.scales;
    let Some(y_col) = space.y.as_ref().and_then(|a| a.data_column()) else {
        return;
    };
    if !space.x.is_band() {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Bar requires a categorical x dimension",
            geo.span,
        ));
        return;
    }
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 1.0);
    let layout = bar_layout(geo);
    let stacked = matches!(layout, BarLayout::Stack | BarLayout::Fill);

    let Some(baseline) = space.map_y(0.0) else {
        return;
    };

    if stacked {
        let Some(x_col) = space.x.data_column() else {
            return;
        };
        let totals = if layout == BarLayout::Fill {
            fill_totals(table, rows, x_col, y_col)
        } else {
            HashMap::new()
        };
        let mut cumulative: HashMap<String, f64> = HashMap::new();
        for row in render_rows(table, rows) {
            let (Some(cx), Some(bw)) = (space.resolve_x(table, row), space.x_bandwidth(table, row))
            else {
                continue;
            };
            let Some(value) = cell_f64(table, y_col, row) else {
                continue;
            };
            let key = crate::scale::cell_category(table, x_col, row).unwrap_or_default();
            let value = if layout == BarLayout::Fill {
                let total = totals.get(&key).copied().unwrap_or(0.0);
                if total.abs() <= f64::EPSILON {
                    continue;
                }
                value / total
            } else {
                value
            };
            let base = *cumulative.get(&key).unwrap_or(&0.0);
            let top = base + value;
            cumulative.insert(key, top);
            let (Some(y0), Some(y1)) = (space.map_y(base), space.map_y(top)) else {
                continue;
            };
            emit_bar(
                w,
                cx - bw / 2.0,
                bw,
                y0,
                y1,
                plot,
                &fill,
                &stroke,
                stroke_width,
                table,
                row,
                alpha,
            );
        }
    } else {
        for row in render_rows(table, rows) {
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
                &stroke,
                stroke_width,
                table,
                row,
                alpha,
            );
        }
    }
}

fn fill_totals(
    table: &dyn Table,
    rows: Option<&[usize]>,
    x_col: &str,
    y_col: &str,
) -> HashMap<String, f64> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    for row in render_rows(table, rows) {
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        let key = cell_category(table, x_col, row).unwrap_or_default();
        *totals.entry(key).or_insert(0.0) += value;
    }
    totals
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
    stroke: &ColorSpec,
    stroke_width: f64,
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
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
        num(x),
        num(top),
        num(width),
        num(bottom - top),
        escape_attr(&color),
        stroke_attrs(stroke, stroke_width, table, row),
        num(alpha),
    ));
}
