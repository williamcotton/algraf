use algraf_data::geo_types::Geometry;
use algraf_data::{
    Column, ColumnDef, DataFrame, DataType, DataValue, DataValueRef, DateTimeValue, Table,
};

use crate::scale::cell_f64;
use crate::svg::num;

use super::util::deterministic_frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDirection {
    Hv,
    Vh,
}

#[derive(Debug, Clone, Copy)]
pub struct StepVerticesOptions {
    pub direction: StepDirection,
}

impl Default for StepVerticesOptions {
    fn default() -> Self {
        StepVerticesOptions {
            direction: StepDirection::Hv,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VectorEndpointsOptions {
    pub length_scale: f64,
}

impl Default for VectorEndpointsOptions {
    fn default() -> Self {
        VectorEndpointsOptions { length_scale: 1.0 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CurveSampleOptions {
    pub curvature: f64,
    pub points: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct JitterPointsOptions {
    pub width: f64,
    pub height: f64,
}

impl Default for CurveSampleOptions {
    fn default() -> Self {
        CurveSampleOptions {
            curvature: 0.35,
            points: 16,
        }
    }
}

impl Default for JitterPointsOptions {
    fn default() -> Self {
        JitterPointsOptions {
            width: 0.0,
            height: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalOrientation {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy)]
pub struct IntervalSegmentsOptions {
    pub orientation: IntervalOrientation,
    pub cap_width: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct IntervalWidthOptions {
    pub orientation: IntervalOrientation,
    pub width: Option<f64>,
}

impl Default for IntervalSegmentsOptions {
    fn default() -> Self {
        IntervalSegmentsOptions {
            orientation: IntervalOrientation::Vertical,
            cap_width: None,
        }
    }
}

impl Default for IntervalWidthOptions {
    fn default() -> Self {
        IntervalWidthOptions {
            orientation: IntervalOrientation::Vertical,
            width: None,
        }
    }
}

/// Expand source-ordered `(x, y)` rows into orthogonal vertices for `Path`.
/// Missing required cells emit a single null sentinel between valid runs, which
/// causes line/path rendering to break rather than connect across the gap.
pub fn step_vertices(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: StepVerticesOptions,
) -> DataFrame {
    let x_dtype = column_dtype(table, x_col);
    let y_dtype = column_dtype(table, y_col);
    let mut xs = ColumnBuilder::new(x_dtype);
    let mut ys = ColumnBuilder::new(y_dtype);
    let mut groups = Vec::new();
    let mut prev: Option<(DataValue, DataValue, i64)> = None;
    let mut group = 0i64;

    for row in 0..table.row_count() {
        let x = owned_cell(table, x_col, row);
        let y = owned_cell(table, y_col, row);
        match (x, y) {
            (Some(x), Some(y)) => {
                if let Some((prev_x, prev_y, current_group)) = &prev {
                    match options.direction {
                        StepDirection::Hv => {
                            push_step_row(&mut xs, &mut ys, &mut groups, &x, prev_y, *current_group)
                        }
                        StepDirection::Vh => {
                            push_step_row(&mut xs, &mut ys, &mut groups, prev_x, &y, *current_group)
                        }
                    }
                    push_step_row(&mut xs, &mut ys, &mut groups, &x, &y, *current_group);
                } else {
                    push_step_row(&mut xs, &mut ys, &mut groups, &x, &y, group);
                }
                prev = Some((x, y, group));
            }
            _ => {
                if prev.is_some() {
                    xs.push_null();
                    ys.push_null();
                    groups.push(None);
                    group += 1;
                }
                prev = None;
            }
        }
    }

    deterministic_frame(
        vec![
            output_col(x_col, x_dtype, true),
            output_col(y_col, y_dtype, true),
            output_col("step_group", DataType::Integer, true),
        ],
        vec![xs.finish(), ys.finish(), Column::from_int_options(groups)],
    )
}

fn push_step_row(
    xs: &mut ColumnBuilder,
    ys: &mut ColumnBuilder,
    groups: &mut Vec<Option<i64>>,
    x: &DataValue,
    y: &DataValue,
    group: i64,
) {
    xs.push_value(Some(x.clone()));
    ys.push_value(Some(y.clone()));
    groups.push(Some(group));
}

/// Produce primitive `Segment` endpoint columns from a start point, angle
/// (radians), and length. Source scalar columns are passed through when their
/// names do not collide with the primitive columns.
pub fn vector_endpoints(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    angle_col: &str,
    length_col: &str,
    options: VectorEndpointsOptions,
) -> DataFrame {
    let passthrough = passthrough_columns(table, &["x", "y", "xend", "yend"]);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut xends = Vec::new();
    let mut yends = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);

    for row in 0..table.row_count() {
        let Some(x) = cell_f64(table, x_col, row) else {
            continue;
        };
        let Some(y) = cell_f64(table, y_col, row) else {
            continue;
        };
        let Some(angle) = cell_f64(table, angle_col, row) else {
            continue;
        };
        let Some(length) = cell_f64(table, length_col, row) else {
            continue;
        };
        let scaled = length * options.length_scale;
        xs.push(Some(x));
        ys.push(Some(y));
        xends.push(Some(x + angle.cos() * scaled));
        yends.push(Some(y + angle.sin() * scaled));
        push_passthrough(table, row, &passthrough, &mut passthrough_builders);
    }

    let mut schema = vec![
        output_col("x", DataType::Float, false),
        output_col("y", DataType::Float, false),
        output_col("xend", DataType::Float, false),
        output_col("yend", DataType::Float, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        Column::from_float_options(xs),
        Column::from_float_options(ys),
        Column::from_float_options(xends),
        Column::from_float_options(yends),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

/// Produce primitive point coordinate columns with deterministic data-space
/// jitter. Source scalar columns are passed through when their names do not
/// collide with the primitive `x`/`y` columns.
pub fn jitter_points(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: JitterPointsOptions,
) -> DataFrame {
    let passthrough = passthrough_columns(table, &["x", "y"]);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);
    for row in 0..table.row_count() {
        let Some(x) = jitter_input(table, x_col, row) else {
            continue;
        };
        let Some(y) = jitter_input(table, y_col, row) else {
            continue;
        };
        xs.push(Some(
            x + options.width.max(0.0) * deterministic_unit(row, 0x9e37_79b9_7f4a_7c15),
        ));
        ys.push(Some(
            y + options.height.max(0.0) * deterministic_unit(row, 0xbf58_476d_1ce4_e5b9),
        ));
        push_passthrough(table, row, &passthrough, &mut passthrough_builders);
    }

    let mut schema = vec![
        output_col("x", DataType::Float, false),
        output_col("y", DataType::Float, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        Column::from_float_options(xs),
        Column::from_float_options(ys),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

/// Emit primitive `Segment` rows for an interval. Vertical intervals use the
/// position column on x and lower/upper values on y; horizontal intervals swap
/// those axes. Optional caps are generated only when the position column is
/// numeric because the derived table is still in data coordinates.
pub fn interval_segments(
    table: &dyn Table,
    position_col: &str,
    lower_col: &str,
    upper_col: &str,
    options: IntervalSegmentsOptions,
) -> DataFrame {
    let position_dtype = interval_coord_dtype(column_dtype(table, position_col));
    let value_dtype = interval_coord_dtype(column_dtype(table, lower_col));
    let (x_dtype, y_dtype) = interval_xy_dtypes(position_dtype, value_dtype, options.orientation);
    let passthrough = passthrough_columns(
        table,
        &["x", "y", "xend", "yend", "interval_role", "interval_id"],
    );
    let mut xs = ColumnBuilder::new(x_dtype);
    let mut ys = ColumnBuilder::new(y_dtype);
    let mut xends = ColumnBuilder::new(x_dtype);
    let mut yends = ColumnBuilder::new(y_dtype);
    let mut roles = Vec::new();
    let mut ids = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);
    let cap_width = options.cap_width.filter(|value| *value > 0.0);

    for row in 0..table.row_count() {
        let Some(position) = typed_cell(table, position_col, row, position_dtype) else {
            continue;
        };
        let Some(lower) = typed_cell(table, lower_col, row, value_dtype) else {
            continue;
        };
        let Some(upper) = typed_cell(table, upper_col, row, value_dtype) else {
            continue;
        };
        push_oriented_segment(
            options.orientation,
            &mut xs,
            &mut ys,
            &mut xends,
            &mut yends,
            &mut roles,
            &mut ids,
            position.clone(),
            lower.clone(),
            position.clone(),
            upper.clone(),
            "stem",
            row as i64,
        );
        push_passthrough(table, row, &passthrough, &mut passthrough_builders);

        if let (Some(width), Some(position_value)) = (cap_width, cell_f64(table, position_col, row))
        {
            let half = width / 2.0;
            let low = position_value - half;
            let high = position_value + half;
            for (role, value) in [("lower_cap", lower), ("upper_cap", upper)] {
                push_cap_segment(
                    options.orientation,
                    &mut xs,
                    &mut ys,
                    &mut xends,
                    &mut yends,
                    &mut roles,
                    &mut ids,
                    low,
                    high,
                    value,
                    role,
                    row as i64,
                );
                push_passthrough(table, row, &passthrough, &mut passthrough_builders);
            }
        }
    }

    let mut schema = vec![
        output_col("x", x_dtype, false),
        output_col("y", y_dtype, false),
        output_col("xend", x_dtype, false),
        output_col("yend", y_dtype, false),
        output_col("interval_role", DataType::String, false),
        output_col("interval_id", DataType::Integer, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        xs.finish(),
        ys.finish(),
        xends.finish(),
        yends.finish(),
        Column::String(roles),
        Column::from_int_options(ids),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

/// Emit primitive `Rect` bounds for interval bodies. Numeric position columns
/// use `width` in data units; categorical positions are passed through for both
/// bounds so `Rect` resolves the category to the full band.
pub fn interval_rects(
    table: &dyn Table,
    position_col: &str,
    lower_col: &str,
    upper_col: &str,
    options: IntervalWidthOptions,
) -> DataFrame {
    let position_dtype = interval_coord_dtype(column_dtype(table, position_col));
    let value_dtype = interval_coord_dtype(column_dtype(table, lower_col));
    let (x_dtype, y_dtype) = interval_xy_dtypes(position_dtype, value_dtype, options.orientation);
    let passthrough = passthrough_columns(
        table,
        &[
            "xmin",
            "xmax",
            "ymin",
            "ymax",
            "interval_role",
            "interval_id",
        ],
    );
    let mut xmins = ColumnBuilder::new(x_dtype);
    let mut xmaxs = ColumnBuilder::new(x_dtype);
    let mut ymins = ColumnBuilder::new(y_dtype);
    let mut ymaxs = ColumnBuilder::new(y_dtype);
    let mut roles = Vec::new();
    let mut ids = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);
    let width = options.width.filter(|value| *value > 0.0).unwrap_or(0.8);

    for row in 0..table.row_count() {
        let Some(position) = typed_cell(table, position_col, row, position_dtype) else {
            continue;
        };
        let Some(lower) = typed_cell(table, lower_col, row, value_dtype) else {
            continue;
        };
        let Some(upper) = typed_cell(table, upper_col, row, value_dtype) else {
            continue;
        };
        let (low_pos, high_pos) = position_bounds(table, position_col, row, position, width);
        match options.orientation {
            IntervalOrientation::Vertical => {
                xmins.push_value(Some(low_pos));
                xmaxs.push_value(Some(high_pos));
                ymins.push_value(Some(lower));
                ymaxs.push_value(Some(upper));
            }
            IntervalOrientation::Horizontal => {
                xmins.push_value(Some(lower));
                xmaxs.push_value(Some(upper));
                ymins.push_value(Some(low_pos));
                ymaxs.push_value(Some(high_pos));
            }
        }
        roles.push(Some("body".to_string()));
        ids.push(Some(row as i64));
        push_passthrough(table, row, &passthrough, &mut passthrough_builders);
    }

    let mut schema = vec![
        output_col("xmin", x_dtype, false),
        output_col("xmax", x_dtype, false),
        output_col("ymin", y_dtype, false),
        output_col("ymax", y_dtype, false),
        output_col("interval_role", DataType::String, false),
        output_col("interval_id", DataType::Integer, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        xmins.finish(),
        xmaxs.finish(),
        ymins.finish(),
        ymaxs.finish(),
        Column::String(roles),
        Column::from_int_options(ids),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

/// Emit primitive `Segment` rows for interval middle lines.
pub fn interval_middles(
    table: &dyn Table,
    position_col: &str,
    middle_col: &str,
    options: IntervalWidthOptions,
) -> DataFrame {
    let position_dtype = interval_coord_dtype(column_dtype(table, position_col));
    let value_dtype = interval_coord_dtype(column_dtype(table, middle_col));
    let (x_dtype, y_dtype) = interval_xy_dtypes(position_dtype, value_dtype, options.orientation);
    let passthrough = passthrough_columns(
        table,
        &["x", "y", "xend", "yend", "interval_role", "interval_id"],
    );
    let mut xs = ColumnBuilder::new(x_dtype);
    let mut ys = ColumnBuilder::new(y_dtype);
    let mut xends = ColumnBuilder::new(x_dtype);
    let mut yends = ColumnBuilder::new(y_dtype);
    let mut roles = Vec::new();
    let mut ids = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);
    let width = options.width.filter(|value| *value > 0.0).unwrap_or(0.8);

    for row in 0..table.row_count() {
        let Some(position) = typed_cell(table, position_col, row, position_dtype) else {
            continue;
        };
        let Some(middle) = typed_cell(table, middle_col, row, value_dtype) else {
            continue;
        };
        let (low_pos, high_pos) = position_bounds(table, position_col, row, position, width);
        push_oriented_segment(
            options.orientation,
            &mut xs,
            &mut ys,
            &mut xends,
            &mut yends,
            &mut roles,
            &mut ids,
            low_pos,
            middle.clone(),
            high_pos,
            middle,
            "middle",
            row as i64,
        );
        push_passthrough(table, row, &passthrough, &mut passthrough_builders);
    }

    let mut schema = vec![
        output_col("x", x_dtype, false),
        output_col("y", y_dtype, false),
        output_col("xend", x_dtype, false),
        output_col("yend", y_dtype, false),
        output_col("interval_role", DataType::String, false),
        output_col("interval_id", DataType::Integer, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        xs.finish(),
        ys.finish(),
        xends.finish(),
        yends.finish(),
        Column::String(roles),
        Column::from_int_options(ids),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

/// Sample one quadratic Bezier-like curve per source row and emit grouped
/// primitive path vertices. Source scalar columns are repeated on every sampled
/// vertex when their names do not collide with the primitive columns.
pub fn curve_sample(
    table: &dyn Table,
    x0_col: &str,
    y0_col: &str,
    x1_col: &str,
    y1_col: &str,
    options: CurveSampleOptions,
) -> DataFrame {
    let points = options.points.max(2);
    let passthrough = passthrough_columns(table, &["x", "y", "link_id"]);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut link_ids = Vec::new();
    let mut passthrough_builders = builders_for_schema(&passthrough);

    for row in 0..table.row_count() {
        let Some(x0) = cell_f64(table, x0_col, row) else {
            continue;
        };
        let Some(y0) = cell_f64(table, y0_col, row) else {
            continue;
        };
        let Some(x1) = cell_f64(table, x1_col, row) else {
            continue;
        };
        let Some(y1) = cell_f64(table, y1_col, row) else {
            continue;
        };
        let (cx, cy) = curve_control_point(x0, y0, x1, y1, options.curvature);
        for i in 0..points {
            let t = i as f64 / (points - 1) as f64;
            let inv = 1.0 - t;
            xs.push(Some(inv * inv * x0 + 2.0 * inv * t * cx + t * t * x1));
            ys.push(Some(inv * inv * y0 + 2.0 * inv * t * cy + t * t * y1));
            link_ids.push(Some(row as i64));
            push_passthrough(table, row, &passthrough, &mut passthrough_builders);
        }
    }

    let mut schema = vec![
        output_col("x", DataType::Float, false),
        output_col("y", DataType::Float, false),
        output_col("link_id", DataType::Integer, false),
    ];
    schema.extend(passthrough);
    let mut columns = vec![
        Column::from_float_options(xs),
        Column::from_float_options(ys),
        Column::from_int_options(link_ids),
    ];
    columns.extend(finish_builders(passthrough_builders));
    deterministic_frame(schema, columns)
}

fn curve_control_point(x0: f64, y0: f64, x1: f64, y1: f64, curvature: f64) -> (f64, f64) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let distance = (dx * dx + dy * dy).sqrt();
    let mid = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    if distance <= f64::EPSILON {
        return mid;
    }
    let normal = (-dy / distance, dx / distance);
    (
        mid.0 + normal.0 * distance * curvature,
        mid.1 + normal.1 * distance * curvature,
    )
}

fn jitter_input(table: &dyn Table, column: &str, row: usize) -> Option<f64> {
    match table.value(column, row)? {
        DataValueRef::Int(value) => Some(value as f64),
        DataValueRef::Float(value) if value.is_finite() => Some(value),
        DataValueRef::Temporal(value) => Some(value.instant.and_utc().timestamp_micros() as f64),
        DataValueRef::Null | DataValueRef::Float(_) => None,
        DataValueRef::Bool(_) | DataValueRef::String(_) | DataValueRef::Geometry(_) => None,
    }
}

fn deterministic_unit(row: usize, salt: u64) -> f64 {
    let mut x = (row as u64).wrapping_add(salt);
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let bits = (x ^ (x >> 31)) >> 11;
    (bits as f64) / 9_007_199_254_740_992.0 - 0.5
}

fn column_dtype(table: &dyn Table, name: &str) -> DataType {
    table
        .schema()
        .iter()
        .find(|column| column.name == name)
        .map(|column| column.dtype)
        .unwrap_or(DataType::Unknown)
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

fn interval_xy_dtypes(
    position_dtype: DataType,
    value_dtype: DataType,
    orientation: IntervalOrientation,
) -> (DataType, DataType) {
    match orientation {
        IntervalOrientation::Vertical => (position_dtype, value_dtype),
        IntervalOrientation::Horizontal => (value_dtype, position_dtype),
    }
}

fn output_col(name: &str, dtype: DataType, nullable: bool) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable,
        examples: vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn push_oriented_segment(
    orientation: IntervalOrientation,
    xs: &mut ColumnBuilder,
    ys: &mut ColumnBuilder,
    xends: &mut ColumnBuilder,
    yends: &mut ColumnBuilder,
    roles: &mut Vec<Option<String>>,
    ids: &mut Vec<Option<i64>>,
    position0: DataValue,
    value0: DataValue,
    position1: DataValue,
    value1: DataValue,
    role: &str,
    id: i64,
) {
    match orientation {
        IntervalOrientation::Vertical => {
            xs.push_value(Some(position0));
            ys.push_value(Some(value0));
            xends.push_value(Some(position1));
            yends.push_value(Some(value1));
        }
        IntervalOrientation::Horizontal => {
            xs.push_value(Some(value0));
            ys.push_value(Some(position0));
            xends.push_value(Some(value1));
            yends.push_value(Some(position1));
        }
    }
    roles.push(Some(role.to_string()));
    ids.push(Some(id));
}

#[allow(clippy::too_many_arguments)]
fn push_cap_segment(
    orientation: IntervalOrientation,
    xs: &mut ColumnBuilder,
    ys: &mut ColumnBuilder,
    xends: &mut ColumnBuilder,
    yends: &mut ColumnBuilder,
    roles: &mut Vec<Option<String>>,
    ids: &mut Vec<Option<i64>>,
    low_position: f64,
    high_position: f64,
    value: DataValue,
    role: &str,
    id: i64,
) {
    push_oriented_segment(
        orientation,
        xs,
        ys,
        xends,
        yends,
        roles,
        ids,
        DataValue::Float(low_position),
        value.clone(),
        DataValue::Float(high_position),
        value,
        role,
        id,
    );
}

fn position_bounds(
    table: &dyn Table,
    position_col: &str,
    row: usize,
    position: DataValue,
    width: f64,
) -> (DataValue, DataValue) {
    if let Some(value) = cell_f64(table, position_col, row) {
        let half = width / 2.0;
        (
            DataValue::Float(value - half),
            DataValue::Float(value + half),
        )
    } else {
        (position.clone(), position)
    }
}

fn typed_cell(table: &dyn Table, column: &str, row: usize, dtype: DataType) -> Option<DataValue> {
    match dtype {
        DataType::Float => cell_f64(table, column, row).map(DataValue::Float),
        DataType::Temporal => match table.value(column, row)? {
            DataValueRef::Temporal(value) => Some(DataValue::Temporal(value)),
            DataValueRef::Null => None,
            _ => None,
        },
        DataType::String
        | DataType::Boolean
        | DataType::Integer
        | DataType::Mixed
        | DataType::Unknown => owned_cell(table, column, row),
        DataType::Geometry => None,
    }
}

fn passthrough_columns(table: &dyn Table, reserved: &[&str]) -> Vec<ColumnDef> {
    table
        .schema()
        .iter()
        .filter(|column| !reserved.contains(&column.name.as_str()))
        .cloned()
        .collect()
}

fn builders_for_schema(schema: &[ColumnDef]) -> Vec<ColumnBuilder> {
    schema
        .iter()
        .map(|column| ColumnBuilder::new(column.dtype))
        .collect()
}

fn push_passthrough(
    table: &dyn Table,
    row: usize,
    schema: &[ColumnDef],
    builders: &mut [ColumnBuilder],
) {
    for (column, builder) in schema.iter().zip(builders.iter_mut()) {
        builder.push_ref(table.value(&column.name, row));
    }
}

fn finish_builders(builders: Vec<ColumnBuilder>) -> Vec<Column> {
    builders.into_iter().map(ColumnBuilder::finish).collect()
}

fn owned_cell(table: &dyn Table, column: &str, row: usize) -> Option<DataValue> {
    match table.value(column, row)? {
        DataValueRef::Null => None,
        value => Some(value.to_owned()),
    }
}

enum ColumnBuilder {
    Bool(Vec<Option<bool>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    Temporal(Vec<Option<DateTimeValue>>),
    String(Vec<Option<String>>),
    Geometry(Vec<Option<Geometry<f64>>>),
}

impl ColumnBuilder {
    fn new(dtype: DataType) -> Self {
        match dtype {
            DataType::Boolean => ColumnBuilder::Bool(Vec::new()),
            DataType::Integer => ColumnBuilder::Int(Vec::new()),
            DataType::Float => ColumnBuilder::Float(Vec::new()),
            DataType::Temporal => ColumnBuilder::Temporal(Vec::new()),
            DataType::Geometry => ColumnBuilder::Geometry(Vec::new()),
            DataType::String | DataType::Mixed | DataType::Unknown => {
                ColumnBuilder::String(Vec::new())
            }
        }
    }

    fn push_ref(&mut self, value: Option<DataValueRef<'_>>) {
        let value = value.and_then(|value| match value {
            DataValueRef::Null => None,
            value => Some(value.to_owned()),
        });
        self.push_value(value);
    }

    fn push_value(&mut self, value: Option<DataValue>) {
        match self {
            ColumnBuilder::Bool(values) => values.push(match value {
                Some(DataValue::Bool(value)) => Some(value),
                _ => None,
            }),
            ColumnBuilder::Int(values) => values.push(match value {
                Some(DataValue::Int(value)) => Some(value),
                _ => None,
            }),
            ColumnBuilder::Float(values) => values.push(match value {
                Some(DataValue::Int(value)) => Some(value as f64),
                Some(DataValue::Float(value)) if value.is_finite() => Some(value),
                _ => None,
            }),
            ColumnBuilder::Temporal(values) => values.push(match value {
                Some(DataValue::Temporal(value)) => Some(value),
                _ => None,
            }),
            ColumnBuilder::String(values) => values.push(value.and_then(value_to_string)),
            ColumnBuilder::Geometry(values) => values.push(match value {
                Some(DataValue::Geometry(value)) => Some(value),
                _ => None,
            }),
        }
    }

    fn push_null(&mut self) {
        self.push_value(None);
    }

    fn finish(self) -> Column {
        match self {
            ColumnBuilder::Bool(values) => Column::from_bool_options(values),
            ColumnBuilder::Int(values) => Column::from_int_options(values),
            ColumnBuilder::Float(values) => Column::from_float_options(values),
            ColumnBuilder::Temporal(values) => Column::from_temporal_options(values),
            ColumnBuilder::String(values) => Column::String(values),
            ColumnBuilder::Geometry(values) => Column::Geometry(values),
        }
    }
}

fn value_to_string(value: DataValue) -> Option<String> {
    match value {
        DataValue::Null | DataValue::Geometry(_) => None,
        DataValue::Bool(value) => Some(value.to_string()),
        DataValue::Int(value) => Some(value.to_string()),
        DataValue::Float(value) if value.is_finite() => Some(num(value)),
        DataValue::Float(_) => None,
        DataValue::Temporal(value) => Some(value.instant.and_utc().to_rfc3339()),
        DataValue::String(value) => Some(value),
    }
}
