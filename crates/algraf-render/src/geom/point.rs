use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::{color_spec, number_setting, number_spec};
use crate::marker::{emit_marker, marker_for_index, parse_marker_shape, MarkerShape};
use crate::scale::cell_category;
use crate::sink::{MarkSink, Paint};

use super::common::{
    adjusted_position, mark_interaction, render_rows, DEFAULT_FILL, DEFAULT_SIZE_RANGE,
};
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
        let (cx, cy) = adjusted_position(geo, space, table, row, cx, cy, true);
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        let s = size.at(table, row, theme.point_size);
        let paint = Paint::fill(&color, Some(alpha));
        sink.begin_mark(mark_interaction(geo, table, row));
        emit_marker(sink, shape.resolve(table, row), cx, cy, s, &paint);
        sink.end_mark();
    }
}

struct ShapeSpec {
    constant: Option<MarkerShape>,
    mapping: Option<(String, Vec<String>)>,
}

impl ShapeSpec {
    fn resolve(&self, table: &dyn Table, row: usize) -> MarkerShape {
        if let Some(shape) = self.constant {
            return shape;
        }
        if let Some((col, categories)) = &self.mapping {
            let Some(category) = cell_category(table, col, row) else {
                return MarkerShape::Circle;
            };
            let index = categories
                .iter()
                .position(|value| value == &category)
                .unwrap_or(0);
            return marker_for_index(index);
        }
        MarkerShape::Circle
    }
}

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
            SettingValue::String(value) => Some(parse_marker_shape(value).unwrap_or_else(|| {
                diagnostics.push(Diagnostic::warning(
                    codes::W2006,
                    format!("unknown point shape `{value}`; using `circle`"),
                    geo.span,
                ));
                MarkerShape::Circle
            })),
            _ => None,
        });
    ShapeSpec {
        constant,
        mapping: None,
    }
}
