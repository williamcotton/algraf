use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::{color_spec, number_setting, number_spec};
use crate::scale::cell_category;
use crate::sink::{MarkSink, Paint};
use crate::svg::num;

use super::common::{mark_interaction, render_rows, DEFAULT_FILL, DEFAULT_SIZE_RANGE};
use super::GeometryRenderContext;

pub(super) fn render(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let size = number_spec(
        geo,
        PropertyKey::Size,
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
        sink.begin_mark(mark_interaction(geo, table, row));
        emit_point_shape(sink, shape.resolve(table, row), cx, cy, s, &color, alpha);
        sink.end_mark();
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
    if let Some(mapping) = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Shape)
    {
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
        .find(|setting| setting.name == PropertyKey::Shape)
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) => match value.as_str() {
                "circle" => Some(PointShape::Circle),
                "square" => Some(PointShape::Square),
                "triangle" => Some(PointShape::Triangle),
                "diamond" => Some(PointShape::Diamond),
                _ => {
                    diagnostics.push(Diagnostic::warning(
                        codes::W2006,
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
    sink: &mut dyn MarkSink,
    shape: PointShape,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    alpha: f64,
) {
    let paint = Paint::fill(color, Some(alpha));
    match shape {
        PointShape::Circle => sink.circle(cx, cy, size, &paint),
        PointShape::Square => {
            let side = size * 2.0;
            sink.rect(cx - size, cy - size, side, side, &paint);
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
            sink.path(&d, &paint);
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
            sink.path(&d, &paint);
        }
    }
}
