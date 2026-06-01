use std::collections::HashSet;

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{DataValueRef, Table};
use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::{number_setting, number_spec};
use crate::geom::{GeometryRenderContext, DEFAULT_SIZE_RANGE};
use crate::sink::MarkSink;

use super::common::{adjusted_position, mark_interaction, opacity_when_translucent, render_rows};

pub(super) fn render(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(src) = source_spec(geo) else {
        return;
    };
    let size = number_spec(
        geo,
        PropertyKey::Size,
        ctx.table,
        ctx.scales,
        DEFAULT_SIZE_RANGE,
        ctx.theme.point_size,
    );
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let mut missing = HashSet::new();
    for row in render_rows(ctx.table, ctx.rows) {
        let (Some(cx), Some(cy)) = (
            ctx.space.resolve_x(ctx.table, row),
            ctx.space.resolve_y(ctx.table, row),
        ) else {
            continue;
        };
        let Some((source, span)) = src.resolve(ctx.table, row) else {
            continue;
        };
        let Some(asset) = ctx.assets.get(source) else {
            if missing.insert(source.to_string()) {
                diagnostics.push(Diagnostic::error(
                    codes::E1204,
                    format!("image source `{source}` was not loaded"),
                    span,
                ));
            }
            continue;
        };
        let (cx, cy) = adjusted_position(geo, ctx.space, ctx.table, row, cx, cy, true);
        let max_side = size.at(ctx.table, row, ctx.theme.point_size).max(0.0);
        if max_side <= 0.0 {
            continue;
        }
        let (width, height) = fit_size(asset.intrinsic_width, asset.intrinsic_height, max_side);
        sink.begin_mark(mark_interaction(geo, ctx.table, row));
        sink.image(
            &asset.href,
            cx - width / 2.0,
            cy - height / 2.0,
            width,
            height,
            opacity_when_translucent(alpha),
        );
        sink.end_mark();
    }
}

fn fit_size(intrinsic_width: f64, intrinsic_height: f64, max_side: f64) -> (f64, f64) {
    if intrinsic_width >= intrinsic_height {
        (max_side, max_side * intrinsic_height / intrinsic_width)
    } else {
        (max_side * intrinsic_width / intrinsic_height, max_side)
    }
}

enum SourceSpec<'a> {
    Constant { value: &'a str, span: Span },
    Mapping { column: &'a str, span: Span },
}

impl<'a> SourceSpec<'a> {
    fn resolve(&self, table: &'a dyn Table, row: usize) -> Option<(&'a str, Span)> {
        match self {
            SourceSpec::Constant { value, span } => Some((*value, *span)),
            SourceSpec::Mapping { column, span } => match table.value(column, row)? {
                DataValueRef::String(value) if !value.is_empty() => Some((value, *span)),
                _ => None,
            },
        }
    }
}

fn source_spec(geo: &GeometryIr) -> Option<SourceSpec<'_>> {
    if let Some(setting) = geo.settings.iter().find(|s| s.name == PropertyKey::Src) {
        if let SettingValue::String(value) = &setting.value {
            return Some(SourceSpec::Constant {
                value,
                span: setting.span,
            });
        }
    }
    geo.mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Src)
        .map(|mapping| SourceSpec::Mapping {
            column: &mapping.column.name,
            span: mapping.span,
        })
}
