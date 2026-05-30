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

impl Default for CurveSampleOptions {
    fn default() -> Self {
        CurveSampleOptions {
            curvature: 0.35,
            points: 16,
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
        vec![xs.finish(), ys.finish(), Column::Int(groups)],
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
        Column::Float(xs),
        Column::Float(ys),
        Column::Float(xends),
        Column::Float(yends),
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
    let mut columns = vec![Column::Float(xs), Column::Float(ys), Column::Int(link_ids)];
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

fn column_dtype(table: &dyn Table, name: &str) -> DataType {
    table
        .schema()
        .iter()
        .find(|column| column.name == name)
        .map(|column| column.dtype)
        .unwrap_or(DataType::Unknown)
}

fn output_col(name: &str, dtype: DataType, nullable: bool) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable,
        examples: vec![],
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
            ColumnBuilder::Bool(values) => Column::Bool(values),
            ColumnBuilder::Int(values) => Column::Int(values),
            ColumnBuilder::Float(values) => Column::Float(values),
            ColumnBuilder::Temporal(values) => Column::Temporal(values),
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
