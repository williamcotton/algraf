//! JSON serialization for the semantic IR.
//!
//! `algraf ir --json` and `algraf schema --json` emit this format. The shape
//! mirrors the IR types defined in `algraf-semantics`; see ALGRAF_SPEC §11
//! for the IR data model.

use algraf_data::DataType;
use algraf_semantics::{
    AestheticMapping, ChartIr, ColumnRef, DataSourceIr, DeriveIr, FrameIr, GeometryIr,
    GeometryKind, GradientIr, GuideOverridesIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, SettingValue,
    SpaceDataRef, SpaceIr, StatKind, StatOptionsIr,
};
use serde_json::{json, Value};

pub(crate) fn ir_to_json(ir: &ChartIr) -> Value {
    json!({
        "dataSource": data_source_json(&ir.data_source),
        "width": ir.width,
        "height": ir.height,
        "layout": {
            "facetColumns": ir.layout.facet_columns,
        },
        "guides": {
            "legend": ir.guides.legend,
            "fillLegend": ir.guides.fill_legend,
            "strokeLegend": ir.guides.stroke_legend,
            "grid": ir.guides.grid,
            "xLabel": ir.guides.x_label.as_deref(),
            "yLabel": ir.guides.y_label.as_deref(),
            "xTimeFormat": ir.guides.x_time_format.as_ref().map(|format| format.as_str()),
            "yTimeFormat": ir.guides.y_time_format.as_ref().map(|format| format.as_str()),
            "xTickLabelAngle": ir.guides.x_tick_label_angle,
            "yTickLabelAngle": ir.guides.y_tick_label_angle,
        },
        "scales": ir.scales.iter().map(scale_json).collect::<Vec<_>>(),
        "title": ir.title.as_deref(),
        "subtitle": ir.subtitle.as_deref(),
        "caption": ir.caption.as_deref(),
        "alt": ir.alt.as_deref(),
        "description": ir.description.as_deref(),
        "metadata": {
            "title": ir.title.as_deref(),
            "subtitle": ir.subtitle.as_deref(),
            "caption": ir.caption.as_deref(),
            "alt": ir.alt.as_deref(),
            "description": ir.description.as_deref(),
        },
        "tables": ir.tables.iter().map(|t| json!({
            "name": t.name,
            "path": t.path,
            "query": t.query.as_deref(),
            "span": span_json(t.span),
        })).collect::<Vec<_>>(),
        "derivedTables": ir.derived_tables.iter().map(derive_json).collect::<Vec<_>>(),
        "spaces": ir.spaces.iter().map(space_json).collect::<Vec<_>>(),
    })
}

fn data_source_json(data_source: &DataSourceIr) -> Value {
    match data_source {
        DataSourceIr::Path(path) => json!({ "kind": "path", "path": path }),
        DataSourceIr::GeoJson(path) => json!({ "kind": "geojson", "path": path }),
        DataSourceIr::Shapefile(path) => json!({ "kind": "shapefile", "path": path }),
        DataSourceIr::Parquet(path) => json!({ "kind": "parquet", "path": path }),
        DataSourceIr::Sqlite { path, query } => {
            json!({ "kind": "sqlite", "path": path, "query": query })
        }
        DataSourceIr::TopoJson { path, object } => {
            json!({ "kind": "topojson", "path": path, "object": object })
        }
        DataSourceIr::Stdin => json!({ "kind": "stdin" }),
        DataSourceIr::Table(name) => json!({ "kind": "table", "name": name }),
        DataSourceIr::Missing => json!({ "kind": "missing" }),
    }
}

fn derive_json(derive: &DeriveIr) -> Value {
    json!({
        "name": derive.name,
        "stat": {
            "kind": stat_kind_str(derive.stat.kind),
            "input": frame_json(&derive.stat.input),
            "options": stat_options_json(&derive.stat.options),
            "span": span_json(derive.stat.span),
        },
        "outputSchema": derive.output_schema.iter().map(|c| {
            json!({ "name": c.name, "type": dtype_str(c.dtype) })
        }).collect::<Vec<_>>(),
        "span": span_json(derive.span),
    })
}

fn space_json(space: &SpaceIr) -> Value {
    json!({
        "data": space_data_json(&space.data),
        "frame": frame_json(&space.frame),
        "guides": guide_overrides_json(&space.guides),
        "scales": space.scales.iter().map(scale_json).collect::<Vec<_>>(),
        "geometries": space.geometries.iter().map(geometry_json).collect::<Vec<_>>(),
        "span": span_json(space.span),
    })
}

fn guide_overrides_json(guides: &GuideOverridesIr) -> Value {
    json!({
        "legend": guides.legend,
        "fillLegend": guides.fill_legend,
        "strokeLegend": guides.stroke_legend,
        "grid": guides.grid,
        "xLabel": guides.x_label.as_deref(),
        "yLabel": guides.y_label.as_deref(),
        "xTimeFormat": guides.x_time_format.as_ref().map(|format| format.as_str()),
        "yTimeFormat": guides.y_time_format.as_ref().map(|format| format.as_str()),
        "xTickLabelAngle": guides.x_tick_label_angle,
        "yTickLabelAngle": guides.y_tick_label_angle,
        "xTickLabelRows": guides.x_tick_label_rows,
        "yTickLabelRows": guides.y_tick_label_rows,
    })
}

fn scale_json(scale: &ScaleIr) -> Value {
    json!({
        "target": scale_target_json(&scale.target),
        "type": scale.scale_type.map(scale_type_str),
        "mode": scale.mode.map(|mode| mode.as_str()),
        "domain": scale.domain,
        "categoricalDomain": scale.categorical_domain.as_ref(),
        "breaks": scale.breaks.as_ref(),
        "tickInterval": scale.tick_interval.map(|interval| {
            json!({ "count": interval.count, "unit": interval.unit.as_str() })
        }),
        "labels": scale.break_labels.as_ref(),
        "expansion": scale.expansion.as_ref().map(|expansion| {
            json!({ "mult": expansion.mult, "add": expansion.add })
        }),
        "range": scale.range,
        "colorRange": scale.color_range.as_ref(),
        "reverse": scale.reverse,
        "integer": scale.integer,
        "palette": scale.palette.as_deref(),
        "gradient": scale.gradient.as_ref().map(gradient_json),
        "colorMap": scale.color_map.as_ref(),
        "labelMap": scale.label_map.as_ref(),
        "label": scale.label.as_deref(),
        "span": span_json(scale.span),
    })
}

fn gradient_json(gradient: &GradientIr) -> Value {
    match gradient {
        GradientIr::Even(stops) => json!({
            "kind": "even",
            "stops": stops,
        }),
        GradientIr::Positioned(stops) => json!({
            "kind": "positioned",
            "stops": stops.iter().map(|stop| {
                json!({ "value": stop.value, "color": stop.color })
            }).collect::<Vec<_>>(),
        }),
    }
}

fn scale_target_json(target: &ScaleTargetIr) -> Value {
    match target {
        ScaleTargetIr::Axis(axis) => json!({
            "kind": "axis",
            "axis": axis.as_str(),
        }),
        ScaleTargetIr::Aesthetic { aesthetic, column } => json!({
            "kind": "aesthetic",
            "aesthetic": aesthetic,
            "column": column.as_ref().map(column_json),
        }),
    }
}

fn scale_type_str(scale_type: ScaleTypeIr) -> &'static str {
    scale_type.as_str()
}

fn space_data_json(data: &SpaceDataRef) -> Value {
    match data {
        SpaceDataRef::Primary => json!({ "kind": "primary" }),
        SpaceDataRef::Derived(name) => json!({ "kind": "derived", "name": name }),
        SpaceDataRef::Table(name) => json!({ "kind": "table", "name": name }),
    }
}

fn geometry_json(geometry: &GeometryIr) -> Value {
    json!({
        "kind": geometry_kind_str(geometry.kind),
        "mappings": geometry.mappings.iter().map(mapping_json).collect::<Vec<_>>(),
        "settings": geometry.settings.iter().map(|s| {
            json!({ "name": s.name.as_str(), "value": setting_value_json(&s.value) })
        }).collect::<Vec<_>>(),
        "interaction": interaction_json(&geometry.interaction),
        "span": span_json(geometry.span),
    })
}

fn interaction_json(interaction: &algraf_semantics::InteractionIr) -> Value {
    json!({
        "tooltip": interaction.tooltip.iter().map(column_json).collect::<Vec<_>>(),
        "highlight": interaction.highlight.as_ref().map(column_json),
        "event": interaction.event.as_ref().map(|event| json!({
            "event": event.event.as_str(),
            "emit": column_json(&event.emit),
        })),
    })
}

fn mapping_json(mapping: &AestheticMapping) -> Value {
    json!({
        "aesthetic": mapping.aesthetic.as_str(),
        "column": column_json(&mapping.column),
    })
}

fn frame_json(frame: &FrameIr) -> Value {
    match frame {
        FrameIr::Vector(column) => json!({ "kind": "vector", "column": column_json(column) }),
        FrameIr::Cartesian(parts) => {
            json!({ "kind": "cartesian", "terms": parts.iter().map(frame_json).collect::<Vec<_>>() })
        }
        FrameIr::Nested { outer, inner } => {
            json!({ "kind": "nested", "outer": frame_json(outer), "inner": frame_json(inner) })
        }
        FrameIr::Union(parts) => {
            json!({ "kind": "union", "terms": parts.iter().map(frame_json).collect::<Vec<_>>() })
        }
        FrameIr::Invalid => json!({ "kind": "invalid" }),
    }
}

fn column_json(column: &ColumnRef) -> Value {
    json!({
        "name": column.name,
        "type": dtype_str(column.dtype),
        "span": span_json(column.span),
    })
}

fn setting_value_json(value: &SettingValue) -> Value {
    match value {
        SettingValue::Number(n) => json!({ "kind": "number", "value": n }),
        SettingValue::String(s) => json!({ "kind": "string", "value": s }),
        SettingValue::Bool(b) => json!({ "kind": "bool", "value": b }),
        SettingValue::Null => json!({ "kind": "null" }),
        SettingValue::NumberArray(values) => json!({ "kind": "numberArray", "value": values }),
    }
}

fn span_json(span: algraf_core::Span) -> Value {
    json!({ "start": span.start, "end": span.end })
}

fn stat_options_json(options: &StatOptionsIr) -> Value {
    match options {
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
            interval,
        } => json!({
            "kind": "bin",
            "bins": bins,
            "binWidth": bin_width,
            "boundary": boundary,
            "closed": closed.as_str(),
            "interval": interval.map(|unit| unit.as_str()),
        }),
        StatOptionsIr::Bin2D { bins } => json!({ "kind": "bin2d", "bins": bins }),
        StatOptionsIr::HexBin { bins } => json!({ "kind": "hexbin", "bins": bins }),
        StatOptionsIr::Summary2D { bins, reducer } => json!({
            "kind": "summary2d",
            "bins": { "x": bins.x, "y": bins.y },
            "reducer": reducer.as_str(),
        }),
        StatOptionsIr::SummaryHex { bins, reducer } => json!({
            "kind": "summaryhex",
            "bins": bins,
            "reducer": reducer.as_str(),
        }),
        StatOptionsIr::ContourLines { levels } => json!({
            "kind": "contourLines",
            "levels": levels_json(levels),
        }),
        StatOptionsIr::ContourBands { levels } => json!({
            "kind": "contourBands",
            "levels": levels_json(levels),
        }),
        StatOptionsIr::Density2D { bandwidth, grid } => json!({
            "kind": "density2d",
            "bandwidth": bandwidth,
            "grid": { "x": grid.x, "y": grid.y },
        }),
        StatOptionsIr::Density2DContours {
            bandwidth,
            grid,
            levels,
        } => json!({
            "kind": "density2dContours",
            "bandwidth": bandwidth,
            "grid": { "x": grid.x, "y": grid.y },
            "levels": levels_json(levels),
        }),
        StatOptionsIr::Density2DBands {
            bandwidth,
            grid,
            levels,
        } => json!({
            "kind": "density2dBands",
            "bandwidth": bandwidth,
            "grid": { "x": grid.x, "y": grid.y },
            "levels": levels_json(levels),
        }),
        StatOptionsIr::Distinct => json!({ "kind": "distinct" }),
        StatOptionsIr::Ecdf => json!({ "kind": "ecdf" }),
        StatOptionsIr::Qq {
            distribution,
            reference,
        } => json!({
            "kind": "qq",
            "distribution": distribution.as_str(),
            "reference": reference,
        }),
        StatOptionsIr::Summary { by, reducer } => json!({
            "kind": "summary",
            "by": by.iter().map(column_json).collect::<Vec<_>>(),
            "reducer": reducer.as_str(),
        }),
        StatOptionsIr::SummaryBin {
            by,
            bins,
            bin_width,
            boundary,
            closed,
            reducer,
        } => json!({
            "kind": "summaryBin",
            "by": by.iter().map(column_json).collect::<Vec<_>>(),
            "bins": bins,
            "binWidth": bin_width,
            "boundary": boundary,
            "closed": closed.as_str(),
            "reducer": reducer.as_str(),
        }),
        StatOptionsIr::Cut {
            breaks,
            labels,
            output,
        } => json!({
            "kind": "cut",
            "breaks": breaks,
            "labels": labels,
            "output": output,
        }),
        StatOptionsIr::Smooth { method, span, se } => json!({
            "kind": "smooth",
            "method": method.as_str(),
            "span": span,
            "se": se,
        }),
        StatOptionsIr::StepVertices { direction } => json!({
            "kind": "stepVertices",
            "direction": direction.as_str(),
        }),
        StatOptionsIr::JitterPoints { width, height } => json!({
            "kind": "jitterPoints",
            "width": width,
            "height": height,
        }),
        StatOptionsIr::VectorEndpoints { length_scale } => json!({
            "kind": "vectorEndpoints",
            "lengthScale": length_scale,
        }),
        StatOptionsIr::CurveSample { curvature, points } => json!({
            "kind": "curveSample",
            "curvature": curvature,
            "points": points,
        }),
        StatOptionsIr::IntervalSegments {
            orientation,
            cap_width,
        } => json!({
            "kind": "intervalSegments",
            "orientation": orientation.as_str(),
            "capWidth": cap_width,
        }),
        StatOptionsIr::IntervalRects { orientation, width } => json!({
            "kind": "intervalRects",
            "orientation": orientation.as_str(),
            "width": width,
        }),
        StatOptionsIr::IntervalMiddles { orientation, width } => json!({
            "kind": "intervalMiddles",
            "orientation": orientation.as_str(),
            "width": width,
        }),
        StatOptionsIr::Density {
            bandwidth,
            grid_points,
        } => json!({
            "kind": "density",
            "bandwidth": bandwidth,
            "gridPoints": grid_points,
        }),
        StatOptionsIr::Count => json!({ "kind": "count" }),
        StatOptionsIr::Centroid => json!({ "kind": "centroid" }),
        StatOptionsIr::Simplify { tolerance } => {
            json!({ "kind": "simplify", "tolerance": tolerance })
        }
        StatOptionsIr::SpatialJoin { table, predicate } => json!({
            "kind": "spatialJoin",
            "table": table,
            "predicate": match predicate {
                algraf_semantics::SpatialPredicateIr::Within => "within",
            },
        }),
    }
}

fn levels_json(levels: &algraf_semantics::LevelSpecIr) -> Value {
    match levels {
        algraf_semantics::LevelSpecIr::Count(count) => {
            json!({ "kind": "count", "count": count })
        }
        algraf_semantics::LevelSpecIr::Values(values) => {
            json!({ "kind": "values", "values": values })
        }
    }
}

fn stat_kind_str(kind: StatKind) -> &'static str {
    kind.display_name()
}

fn geometry_kind_str(kind: GeometryKind) -> &'static str {
    kind.display_name()
}

pub(crate) fn dtype_str(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Geometry => "geometry",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}
