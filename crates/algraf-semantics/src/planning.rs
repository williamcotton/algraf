//! Schema-only planning helpers for built-in stats.
//!
//! These functions describe the columns a stat will produce from an already
//! typed input frame. They do not inspect row values or execute transforms, so
//! the analyzer and LSP can make derived-table schemas available before the
//! renderer materializes any data (spec §10.6, §24.2).

use algraf_data::DataType;

use crate::ir::{ColumnDefIr, ColumnRef, FrameIr, IntervalOrientationIr, StatKind};

/// Return the output schema for a built-in stat from its typed input frame.
pub fn stat_output_schema(kind: StatKind, input: &FrameIr) -> Vec<ColumnDefIr> {
    match kind {
        StatKind::Bin => match input {
            FrameIr::Vector(column) => bin_output_schema(column.dtype),
            FrameIr::Union(_) => blended_bin_output_schema(),
            _ => bin_output_schema(DataType::Float),
        },
        StatKind::Bin2D => bin2d_output_schema(),
        StatKind::HexBin => hexbin_output_schema(),
        StatKind::Summary2D => summary2d_output_schema(),
        StatKind::SummaryHex => summaryhex_output_schema(),
        StatKind::ContourLines => contour_lines_output_schema(),
        StatKind::ContourBands => contour_bands_output_schema(),
        StatKind::Density2D => density2d_output_schema(),
        StatKind::Density2DContours => density2d_contours_output_schema(),
        StatKind::Density2DBands => density2d_bands_output_schema(),
        StatKind::Distinct => Vec::new(),
        StatKind::Ecdf => ecdf_output_schema(),
        StatKind::Qq => qq_output_schema(),
        StatKind::Summary => summary_output_schema(&[], false),
        StatKind::SummaryBin => summarybin_output_schema(&[], false),
        StatKind::Cut => Vec::new(),
        // The plain (no-`se`) schema; the analyzer rebuilds with bands when the
        // `se` option is set (spec §15.x).
        StatKind::Smooth => smooth_output_schema(false),
        StatKind::JitterPoints => jitter_points_output_schema(),
        StatKind::StepVertices => match input {
            FrameIr::Cartesian(columns) => {
                let x_dtype = vector_dtype(columns.first());
                let y_dtype = vector_dtype(columns.get(1));
                step_vertices_output_schema(
                    vector_name(columns.first()).unwrap_or("x"),
                    x_dtype,
                    vector_name(columns.get(1)).unwrap_or("y"),
                    y_dtype,
                )
            }
            _ => step_vertices_output_schema("x", DataType::Float, "y", DataType::Float),
        },
        StatKind::VectorEndpoints => vector_endpoints_output_schema(),
        StatKind::CurveSample => curve_sample_output_schema(),
        StatKind::IntervalSegments => {
            interval_segments_output_schema(input, IntervalOrientationIr::Vertical)
        }
        StatKind::IntervalRects => {
            interval_rects_output_schema(input, IntervalOrientationIr::Vertical)
        }
        StatKind::IntervalMiddles => {
            interval_middles_output_schema(input, IntervalOrientationIr::Vertical)
        }
        StatKind::Density => match input {
            FrameIr::Vector(_) => density_output_schema(),
            FrameIr::Union(_) => blended_density_output_schema(),
            _ => density_output_schema(),
        },
        StatKind::Count => count_output_schema(&frame_group_columns(input)),
        StatKind::Boxplot => Vec::new(),
        // Geometry-producing stats pass scalar columns through, so their real
        // schema is built from the upstream table in the analyzer (spec §15.13).
        // Here only the produced geometry column is known from the input frame.
        StatKind::Centroid | StatKind::Simplify | StatKind::SpatialJoin => match input {
            FrameIr::Vector(column) => vec![ColumnDefIr {
                name: column.name.clone(),
                dtype: DataType::Geometry,
            }],
            _ => Vec::new(),
        },
    }
}

pub fn jitter_points_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for an empirical CDF vertex table.
pub fn ecdf_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for normal QQ plot points plus optional reference-line columns.
pub fn qq_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "theoretical".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "sample".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "line_x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "line_y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "line_xend".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "line_yend".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "role".into(),
            dtype: DataType::String,
        },
    ]
}

/// Output schema for grouped one-dimensional summaries.
pub fn summary_output_schema(groups: &[ColumnDefIr], has_bounds: bool) -> Vec<ColumnDefIr> {
    let mut schema = groups.to_vec();
    schema.extend(summary_measure_schema(has_bounds));
    schema
}

/// Output schema for binned one-dimensional summaries.
pub fn summarybin_output_schema(groups: &[ColumnDefIr], has_bounds: bool) -> Vec<ColumnDefIr> {
    let mut schema = vec![
        ColumnDefIr {
            name: "bin_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_center".into(),
            dtype: DataType::Float,
        },
    ];
    schema.extend_from_slice(groups);
    schema.extend(summary_measure_schema(has_bounds));
    schema
}

fn summary_measure_schema(has_bounds: bool) -> Vec<ColumnDefIr> {
    let mut schema = vec![
        ColumnDefIr {
            name: "value".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
    ];
    if has_bounds {
        schema.extend([
            ColumnDefIr {
                name: "lower".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "upper".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "se".into(),
                dtype: DataType::Float,
            },
        ]);
    }
    schema
}

fn vector_name(frame: Option<&FrameIr>) -> Option<&str> {
    match frame {
        Some(FrameIr::Vector(column)) => Some(&column.name),
        _ => None,
    }
}

fn vector_dtype(frame: Option<&FrameIr>) -> DataType {
    match frame {
        Some(FrameIr::Vector(column)) => column.dtype,
        _ => DataType::Float,
    }
}

/// Output schema for one-dimensional binning.
pub fn bin_output_schema(input_dtype: DataType) -> Vec<ColumnDefIr> {
    let boundary_dtype = bin_boundary_dtype(input_dtype);
    vec![
        ColumnDefIr {
            name: "bin_start".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "bin_end".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "bin_center".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for a grouped histogram bin (spec §15.6): the per-bin columns,
/// the group key column (preserving its name), and the pre-stacked y-bounds.
pub fn grouped_bin_output_schema(group_name: &str) -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "bin_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: group_name.into(),
            dtype: DataType::String,
        },
        ColumnDefIr {
            name: "stack_lower".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "stack_upper".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "dodge_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "dodge_end".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for a blended histogram bin (spec §15.6): the per-bin columns
/// plus a synthetic `series` key naming the source column for each member.
pub fn blended_bin_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "bin_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "bin_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "series".into(),
            dtype: DataType::String,
        },
    ]
}

/// Boundary columns stay temporal only when the source column is temporal.
pub fn bin_boundary_dtype(input_dtype: DataType) -> DataType {
    if input_dtype == DataType::Temporal {
        DataType::Temporal
    } else {
        DataType::Float
    }
}

/// Output schema for smoothing. With `se`, confidence-band columns `ymin`,
/// `ymax`, and `se` follow the fitted `x`/`y` (spec §15.x).
pub fn smooth_output_schema(se: bool) -> Vec<ColumnDefIr> {
    let mut schema = vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
    ];
    if se {
        schema.extend([
            ColumnDefIr {
                name: "ymin".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "ymax".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "se".into(),
                dtype: DataType::Float,
            },
        ]);
    }
    schema
}

/// Output schema for step-vertex expansion.
pub fn step_vertices_output_schema(
    x_name: &str,
    x_dtype: DataType,
    y_name: &str,
    y_dtype: DataType,
) -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: x_name.into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: y_name.into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "step_group".into(),
            dtype: DataType::Integer,
        },
    ]
}

/// Output schema for vector endpoint construction. The renderer appends
/// non-conflicting source columns at execution time; these four primitive
/// columns are always available during analysis.
pub fn vector_endpoints_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "xend".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "yend".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for sampled curve vertices.
pub fn curve_sample_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "link_id".into(),
            dtype: DataType::Integer,
        },
    ]
}

/// Output schema for interval segment endpoints. A vertical interval uses
/// position values on x and lower/upper values on y; horizontal swaps them.
pub fn interval_segments_output_schema(
    input: &FrameIr,
    orientation: IntervalOrientationIr,
) -> Vec<ColumnDefIr> {
    let (position_dtype, value_dtype) = interval_input_dtypes(input, 3);
    let position_dtype = interval_coord_dtype(position_dtype);
    let value_dtype = interval_coord_dtype(value_dtype);
    let (x_dtype, y_dtype) = match orientation {
        IntervalOrientationIr::Vertical => (position_dtype, value_dtype),
        IntervalOrientationIr::Horizontal => (value_dtype, position_dtype),
    };
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "xend".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "yend".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "interval_role".into(),
            dtype: DataType::String,
        },
        ColumnDefIr {
            name: "interval_id".into(),
            dtype: DataType::Integer,
        },
    ]
}

/// Output schema for interval rectangle bounds.
pub fn interval_rects_output_schema(
    input: &FrameIr,
    orientation: IntervalOrientationIr,
) -> Vec<ColumnDefIr> {
    let (position_dtype, value_dtype) = interval_input_dtypes(input, 3);
    let position_dtype = interval_coord_dtype(position_dtype);
    let value_dtype = interval_coord_dtype(value_dtype);
    let (x_dtype, y_dtype) = match orientation {
        IntervalOrientationIr::Vertical => (position_dtype, value_dtype),
        IntervalOrientationIr::Horizontal => (value_dtype, position_dtype),
    };
    interval_rects_fixed_schema(x_dtype, y_dtype)
}

/// Output schema for interval middle-line endpoints.
pub fn interval_middles_output_schema(
    input: &FrameIr,
    orientation: IntervalOrientationIr,
) -> Vec<ColumnDefIr> {
    let (position_dtype, value_dtype) = interval_input_dtypes(input, 2);
    let position_dtype = interval_coord_dtype(position_dtype);
    let value_dtype = interval_coord_dtype(value_dtype);
    let (x_dtype, y_dtype) = match orientation {
        IntervalOrientationIr::Vertical => (position_dtype, value_dtype),
        IntervalOrientationIr::Horizontal => (value_dtype, position_dtype),
    };
    interval_middles_fixed_schema(x_dtype, y_dtype)
}

fn interval_rects_fixed_schema(x_dtype: DataType, y_dtype: DataType) -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "xmin".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "xmax".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "ymin".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "ymax".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "interval_role".into(),
            dtype: DataType::String,
        },
        ColumnDefIr {
            name: "interval_id".into(),
            dtype: DataType::Integer,
        },
    ]
}

fn interval_middles_fixed_schema(x_dtype: DataType, y_dtype: DataType) -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "xend".into(),
            dtype: x_dtype,
        },
        ColumnDefIr {
            name: "yend".into(),
            dtype: y_dtype,
        },
        ColumnDefIr {
            name: "interval_role".into(),
            dtype: DataType::String,
        },
        ColumnDefIr {
            name: "interval_id".into(),
            dtype: DataType::Integer,
        },
    ]
}

fn interval_input_dtypes(input: &FrameIr, expected: usize) -> (DataType, DataType) {
    match input {
        FrameIr::Cartesian(columns) if columns.len() >= expected => {
            (vector_dtype(columns.first()), vector_dtype(columns.get(1)))
        }
        _ => (DataType::Float, DataType::Float),
    }
}

fn interval_coord_dtype(dtype: DataType) -> DataType {
    match dtype {
        DataType::Integer | DataType::Float => DataType::Float,
        DataType::Temporal => DataType::Temporal,
        DataType::Geometry => DataType::Unknown,
        DataType::Boolean | DataType::String | DataType::Mixed | DataType::Unknown => {
            DataType::String
        }
    }
}

/// Output schema for rectangular two-dimensional bins.
pub fn bin2d_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "x_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "x_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for hexagonal bins.
pub fn hexbin_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "radius".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for rectangular x/y/z summary bins.
pub fn summary2d_output_schema() -> Vec<ColumnDefIr> {
    let mut schema = bin2d_output_schema();
    schema.push(ColumnDefIr {
        name: "value".into(),
        dtype: DataType::Float,
    });
    schema
}

/// Output schema for hexagonal x/y/z summary bins. The geometry column allows
/// explicit hex summaries to render through `Geo(fill: value)`.
pub fn summaryhex_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "geom".into(),
            dtype: DataType::Geometry,
        },
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "radius".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_radius".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "value".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for line-contour vertices. Rows are emitted in groups that can
/// be rendered with `Path(group: contour_id, stroke: level)`.
pub fn contour_lines_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "level".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "level_index".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "contour_id".into(),
            dtype: DataType::Integer,
        },
    ]
}

/// Output schema for filled contour bands as geometry rows.
pub fn contour_bands_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "geom".into(),
            dtype: DataType::Geometry,
        },
        ColumnDefIr {
            name: "level_low".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "level_high".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "level_mid".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "band_index".into(),
            dtype: DataType::Integer,
        },
    ]
}

/// Output schema for a 2D kernel-density grid.
pub fn density2d_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for contour lines over a 2D density grid.
pub fn density2d_contours_output_schema() -> Vec<ColumnDefIr> {
    contour_lines_output_schema()
}

/// Output schema for filled bands over a 2D density grid.
pub fn density2d_bands_output_schema() -> Vec<ColumnDefIr> {
    contour_bands_output_schema()
}

/// Output schema for kernel density estimation.
pub fn density_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "density_x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

/// Output schema for blended kernel density estimation.
pub fn blended_density_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "density_x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "series".into(),
            dtype: DataType::String,
        },
    ]
}

/// Output schema for count aggregation over one or two grouping columns.
pub fn count_output_schema(group_columns: &[ColumnRef]) -> Vec<ColumnDefIr> {
    let mut output: Vec<ColumnDefIr> = group_columns
        .iter()
        .map(|column| ColumnDefIr {
            name: column.name.clone(),
            dtype: column.dtype,
        })
        .collect();
    output.push(ColumnDefIr {
        name: "count".into(),
        dtype: DataType::Integer,
    });
    output
}

/// The name of the first geometry column in a `(name, dtype)` schema, if any.
/// Used by spatial joins to locate the polygon side's geometry (spec §15.14).
pub fn geometry_column_name<'a>(
    schema: impl IntoIterator<Item = (&'a str, DataType)>,
) -> Option<String> {
    schema
        .into_iter()
        .find(|(_, dtype)| *dtype == DataType::Geometry)
        .map(|(name, _)| name.to_string())
}

/// The polygon-side columns a spatial join appends to each point row (spec
/// §15.14): every non-geometry polygon column whose name does not already exist
/// on the point side. The rule is shared by the analyzer (schema planning) and
/// the renderer (execution) so both agree on the output columns.
pub fn spatial_join_appended_columns<'a>(
    point_names: impl IntoIterator<Item = &'a str>,
    polygon: impl IntoIterator<Item = (&'a str, DataType)>,
) -> Vec<ColumnDefIr> {
    let existing: std::collections::HashSet<&str> = point_names.into_iter().collect();
    polygon
        .into_iter()
        .filter(|(name, dtype)| *dtype != DataType::Geometry && !existing.contains(name))
        .map(|(name, dtype)| ColumnDefIr {
            name: name.to_string(),
            dtype,
        })
        .collect()
}

fn frame_group_columns(input: &FrameIr) -> Vec<ColumnRef> {
    match input {
        FrameIr::Vector(column) => vec![column.clone()],
        FrameIr::Nested { outer, inner } => match (outer.as_ref(), inner.as_ref()) {
            (FrameIr::Vector(outer), FrameIr::Vector(inner)) => {
                vec![outer.clone(), inner.clone()]
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}
