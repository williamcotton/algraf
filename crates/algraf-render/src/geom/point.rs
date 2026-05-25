use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{GeometryIr, SettingValue};

use crate::aes::{color_spec, number_setting, number_spec};
use crate::scale::cell_category;
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{render_rows, DEFAULT_FILL, DEFAULT_SIZE_RANGE};
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
    let theme = ctx.theme;
    let scales = ctx.scales;
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_spec(
        geo,
        "size",
        table,
        scales,
        DEFAULT_SIZE_RANGE,
        theme.point_size,
    );
    let shape = shape_spec(geo, table, diagnostics);
    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        let s = size.at(table, row, theme.point_size);
        emit_point_shape(w, shape.resolve(table, row), cx, cy, s, &color, alpha);
    }
}

#[derive(Debug, Clone, Copy)]
enum PointShape {
    Circle,
    Square,
    Triangle,
    Diamond,
}

struct ShapeSpec {
    constant: Option<PointShape>,
    mapping: Option<(String, Vec<String>)>,
}

impl ShapeSpec {
    fn resolve(&self, table: &dyn Table, row: usize) -> PointShape {
        if let Some(shape) = self.constant {
            return shape;
        }
        if let Some((col, categories)) = &self.mapping {
            let Some(category) = cell_category(table, col, row) else {
                return PointShape::Circle;
            };
            let index = categories
                .iter()
                .position(|value| value == &category)
                .unwrap_or(0);
            return SHAPES[index % SHAPES.len()];
        }
        PointShape::Circle
    }
}

const SHAPES: &[PointShape] = &[
    PointShape::Circle,
    PointShape::Square,
    PointShape::Triangle,
    PointShape::Diamond,
];

fn shape_spec(geo: &GeometryIr, table: &dyn Table, diagnostics: &mut Vec<Diagnostic>) -> ShapeSpec {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == "shape") {
        return ShapeSpec {
            constant: None,
            mapping: Some((
                mapping.column.name.clone(),
                crate::scale::categorical_domain(table, &mapping.column.name),
            )),
        };
    }
    let constant = geo
        .settings
        .iter()
        .find(|setting| setting.name == "shape")
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) => match value.as_str() {
                "circle" => Some(PointShape::Circle),
                "square" => Some(PointShape::Square),
                "triangle" => Some(PointShape::Triangle),
                "diamond" => Some(PointShape::Diamond),
                _ => {
                    diagnostics.push(Diagnostic::warning(
                        "W2006",
                        format!("unknown point shape `{value}`; using `circle`"),
                        geo.span,
                    ));
                    Some(PointShape::Circle)
                }
            },
            _ => None,
        });
    ShapeSpec {
        constant,
        mapping: None,
    }
}

fn emit_point_shape(
    w: &mut SvgWriter,
    shape: PointShape,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    alpha: f64,
) {
    match shape {
        PointShape::Circle => w.line(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(cx),
            num(cy),
            num(size),
            escape_attr(color),
            num(alpha),
        )),
        PointShape::Square => {
            let side = size * 2.0;
            w.line(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                num(cx - size),
                num(cy - size),
                num(side),
                num(side),
                escape_attr(color),
                num(alpha),
            ));
        }
        PointShape::Triangle => {
            let d = format!(
                "M{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy + size),
                num(cx - size),
                num(cy + size)
            );
            w.line(&format!(
                "<path d=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                d,
                escape_attr(color),
                num(alpha),
            ));
        }
        PointShape::Diamond => {
            let d = format!(
                "M{} {} L{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy),
                num(cx),
                num(cy + size),
                num(cx - size),
                num(cy)
            );
            w.line(&format!(
                "<path d=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                d,
                escape_attr(color),
                num(alpha),
            ));
        }
    }
}
