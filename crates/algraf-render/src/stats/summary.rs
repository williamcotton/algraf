use std::collections::BTreeMap;

use algraf_data::geo_types::Geometry;
use algraf_data::{
    Column, ColumnDef, ColumnView, DataFrame, DataType, DataValue, DataValueRef, DateTimeValue,
    Table,
};

use crate::scale::{categorical_domain, cell_category, cell_f64};
use crate::svg::num;

use super::bin::{bin_index, bin_layout};
use super::density::percentile;
use super::util::{col_def, deterministic_frame};
use super::BinOptions;

/// Reducers shared by one-dimensional and binned summary stats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SummaryReducer {
    Count,
    #[default]
    Mean,
    Min,
    Max,
    Sum,
    Median,
    MeanSe,
}

#[derive(Debug, Clone, Copy)]
pub struct QqOptions {
    pub reference: bool,
}

impl Default for QqOptions {
    fn default() -> Self {
        QqOptions { reference: true }
    }
}

/// Compute a count derived table grouping rows by one or two categorical
/// columns (spec §15.5). Output columns are the group keys (preserving input
/// type) followed by an integer `count` column.
pub fn count_by(table: &dyn Table, group_columns: &[&str]) -> DataFrame {
    assert!(
        !group_columns.is_empty(),
        "count_by requires a group column"
    );
    let outer = group_columns[0];
    let inner = group_columns.get(1).copied();
    let outer_cats = categorical_domain(table, outer);
    let outer_view = table.column(outer);

    let mut rows: Vec<(String, Option<String>, i64)> = Vec::new();
    if let Some(inner_col) = inner {
        let inner_cats = categorical_domain(table, inner_col);
        let inner_view = table.column(inner_col);
        let mut counts: BTreeMap<(String, String), i64> = BTreeMap::new();
        for row in 0..table.row_count() {
            let (Some(outer_key), Some(inner_key)) = (
                category_cell(outer_view, table, outer, row),
                category_cell(inner_view, table, inner_col, row),
            ) else {
                continue;
            };
            *counts.entry((outer_key, inner_key)).or_insert(0) += 1;
        }
        for o in &outer_cats {
            for i in &inner_cats {
                let count = *counts.get(&(o.clone(), i.clone())).unwrap_or(&0);
                rows.push((o.clone(), Some(i.clone()), count));
            }
        }
    } else {
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for row in 0..table.row_count() {
            let Some(outer_key) = category_cell(outer_view, table, outer, row) else {
                continue;
            };
            *counts.entry(outer_key).or_insert(0) += 1;
        }
        for o in &outer_cats {
            let count = *counts.get(o).unwrap_or(&0);
            rows.push((o.clone(), None, count));
        }
    }

    let mut schema = vec![col_def(outer, DataType::String)];
    let outer_col = Column::String(rows.iter().map(|r| Some(r.0.clone())).collect());
    let mut columns = vec![outer_col];
    if let Some(inner_col) = inner {
        schema.push(col_def(inner_col, DataType::String));
        columns.push(Column::String(rows.iter().map(|r| r.1.clone()).collect()));
    }
    schema.push(col_def("count", DataType::Integer));
    columns.push(Column::from_int_options(
        rows.iter().map(|r| Some(r.2)).collect(),
    ));
    deterministic_frame(schema, columns)
}

fn category_cell(
    view: Option<ColumnView<'_>>,
    table: &dyn Table,
    column: &str,
    row: usize,
) -> Option<String> {
    match view {
        Some(view) => view.category_at(row),
        None => cell_category(table, column, row),
    }
}

/// Retain the first row for each distinct key tuple. Missing key values are
/// valid key members, so two missing values compare equal.
pub fn distinct(table: &dyn Table, key_columns: &[&str]) -> DataFrame {
    let schema = table.schema().to_vec();
    let mut builders = builders_for_schema(&schema);
    let mut seen: BTreeMap<Vec<DataValue>, ()> = BTreeMap::new();
    for row in 0..table.row_count() {
        let key: Vec<DataValue> = key_columns
            .iter()
            .map(|column| {
                table
                    .value(column, row)
                    .map(|value| value.to_owned())
                    .unwrap_or(DataValue::Null)
            })
            .collect();
        if seen.insert(key, ()).is_some() {
            continue;
        }
        push_passthrough(table, row, &schema, &mut builders);
    }
    deterministic_frame(schema, finish_builders(builders))
}

/// Empirical CDF step vertices. Missing/non-numeric values are skipped. The
/// output starts at `(min(x), 0)` and is right-continuous: for each unique x it
/// emits a vertical jump from the previous cumulative share to the new share.
pub fn ecdf(table: &dyn Table, input_column: &str) -> DataFrame {
    let schema = vec![col_def("x", DataType::Float), col_def("y", DataType::Float)];
    let mut values: Vec<f64> = (0..table.row_count())
        .filter_map(|row| cell_f64(table, input_column, row))
        .collect();
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return deterministic_frame(
            schema,
            vec![
                Column::from_float_options(vec![]),
                Column::from_float_options(vec![]),
            ],
        );
    }
    let n = values.len() as f64;
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut i = 0usize;
    let mut previous = 0.0;
    while i < values.len() {
        let x = values[i];
        let mut j = i + 1;
        while j < values.len() && values[j].total_cmp(&x).is_eq() {
            j += 1;
        }
        let next = j as f64 / n;
        xs.push(Some(x));
        ys.push(Some(previous));
        xs.push(Some(x));
        ys.push(Some(next));
        previous = next;
        i = j;
    }
    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(xs),
            Column::from_float_options(ys),
        ],
    )
}

/// Normal QQ plot points plus optional reference-line endpoints. Point rows
/// carry `theoretical`/`sample`; the reference row carries `line_*` columns.
pub fn qq(table: &dyn Table, input_column: &str, options: QqOptions) -> DataFrame {
    let schema = qq_schema();
    let mut values: Vec<f64> = (0..table.row_count())
        .filter_map(|row| cell_f64(table, input_column, row))
        .collect();
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return empty_qq_frame(schema);
    }
    let n = values.len();
    let mut theoretical = Vec::new();
    let mut sample = Vec::new();
    let mut line_x = Vec::new();
    let mut line_y = Vec::new();
    let mut line_xend = Vec::new();
    let mut line_yend = Vec::new();
    let mut role = Vec::new();
    for (i, value) in values.iter().copied().enumerate() {
        let p = (i as f64 + 0.5) / n as f64;
        theoretical.push(Some(normal_quantile(p)));
        sample.push(Some(value));
        line_x.push(None);
        line_y.push(None);
        line_xend.push(None);
        line_yend.push(None);
        role.push(Some("point".to_string()));
    }
    if options.reference && n >= 2 {
        let q1 = percentile(&values, 0.25);
        let q3 = percentile(&values, 0.75);
        let z1 = normal_quantile(0.25);
        let z3 = normal_quantile(0.75);
        let slope = if (z3 - z1).abs() > f64::EPSILON {
            (q3 - q1) / (z3 - z1)
        } else {
            1.0
        };
        let intercept = q1 - slope * z1;
        let x0 = theoretical.iter().flatten().copied().next().unwrap_or(0.0);
        let x1 = theoretical.iter().flatten().copied().last().unwrap_or(0.0);
        theoretical.push(None);
        sample.push(None);
        line_x.push(Some(x0));
        line_y.push(Some(intercept + slope * x0));
        line_xend.push(Some(x1));
        line_yend.push(Some(intercept + slope * x1));
        role.push(Some("reference".to_string()));
    }
    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(theoretical),
            Column::from_float_options(sample),
            Column::from_float_options(line_x),
            Column::from_float_options(line_y),
            Column::from_float_options(line_xend),
            Column::from_float_options(line_yend),
            Column::String(role),
        ],
    )
}

/// Grouped summary over one value column.
pub fn summary(
    table: &dyn Table,
    value_column: &str,
    by_columns: &[&str],
    reducer: SummaryReducer,
) -> DataFrame {
    let groups = grouped_values(table, value_column, by_columns, reducer);
    let schema = summary_schema(table, by_columns, reducer == SummaryReducer::MeanSe);
    let mut builders = builders_for_schema(&schema);
    for group in groups {
        push_group_key(&group.key, &mut builders);
        push_measure(&group.values, reducer, &mut builders[by_columns.len()..]);
    }
    deterministic_frame(schema, finish_builders(builders))
}

/// Binned summary over a continuous x axis and one value column.
pub fn summary_bin(
    table: &dyn Table,
    x_column: &str,
    value_column: &str,
    by_columns: &[&str],
    options: BinOptions,
    reducer: SummaryReducer,
) -> DataFrame {
    let bins = options.bins.max(1);
    let (min, max) = crate::scale::numeric_domain(table, x_column).unwrap_or((0.0, 1.0));
    let (start, width, bin_count) = bin_layout(min, max, bins, options);
    let group_keys = group_key_domain(table, by_columns, x_column, Some(value_column));
    let group_count = group_keys.len().max(1);
    let mut cells = vec![Vec::new(); bin_count * group_count];
    for row in 0..table.row_count() {
        let Some(x) = cell_f64(table, x_column, row) else {
            continue;
        };
        let Some(value) = summary_value(table, value_column, row, reducer) else {
            continue;
        };
        let Some(key) = group_key(table, by_columns, row) else {
            continue;
        };
        let Some(gi) = group_keys.iter().position(|existing| existing == &key) else {
            continue;
        };
        let bi = bin_index(x, start, width, bin_count, options.closed);
        cells[bi * group_count + gi].push(value);
    }

    let mut schema = vec![
        col_def("bin_start", DataType::Float),
        col_def("bin_end", DataType::Float),
        col_def("bin_center", DataType::Float),
    ];
    schema.extend(group_schema(table, by_columns));
    schema.extend(measure_schema(reducer == SummaryReducer::MeanSe));
    let mut builders = builders_for_schema(&schema);
    for bi in 0..bin_count {
        let bin_start = start + bi as f64 * width;
        let bin_end = bin_start + width;
        for (gi, key) in group_keys.iter().enumerate() {
            let cell = &cells[bi * group_count + gi];
            if cell.is_empty() && reducer != SummaryReducer::Count {
                continue;
            }
            builders[0].push_value(Some(DataValue::Float(bin_start)));
            builders[1].push_value(Some(DataValue::Float(bin_end)));
            builders[2].push_value(Some(DataValue::Float((bin_start + bin_end) / 2.0)));
            push_group_key(key, &mut builders[3..]);
            let offset = 3 + by_columns.len();
            push_measure(cell, reducer, &mut builders[offset..]);
        }
    }
    deterministic_frame(schema, finish_builders(builders))
}

/// Class a continuous value column into reusable labeled bins and append the
/// class column to the original table.
pub fn cut(
    table: &dyn Table,
    input_column: &str,
    breaks: &[f64],
    labels: Option<&[String]>,
    output_column: &str,
) -> DataFrame {
    let mut schema = table.schema().to_vec();
    schema.push(col_def(output_column, DataType::String));
    let mut builders = builders_for_schema(&schema);
    for row in 0..table.row_count() {
        push_passthrough(
            table,
            row,
            table.schema(),
            &mut builders[..table.schema().len()],
        );
        let class = cell_f64(table, input_column, row)
            .and_then(|value| cut_index(value, breaks))
            .map(|index| cut_label(index, breaks, labels));
        builders[table.schema().len()].push_value(class.map(DataValue::String));
    }
    deterministic_frame(schema, finish_builders(builders))
}

#[derive(Debug)]
struct GroupValues {
    key: Vec<DataValue>,
    values: Vec<f64>,
}

fn grouped_values(
    table: &dyn Table,
    value_column: &str,
    by_columns: &[&str],
    reducer: SummaryReducer,
) -> Vec<GroupValues> {
    let mut groups: BTreeMap<Vec<DataValue>, Vec<f64>> = BTreeMap::new();
    for row in 0..table.row_count() {
        let Some(value) = summary_value(table, value_column, row, reducer) else {
            continue;
        };
        let Some(key) = group_key(table, by_columns, row) else {
            continue;
        };
        groups.entry(key).or_default().push(value);
    }
    if by_columns.is_empty() && groups.is_empty() && reducer == SummaryReducer::Count {
        groups.insert(Vec::new(), Vec::new());
    }
    groups
        .into_iter()
        .map(|(key, values)| GroupValues { key, values })
        .collect()
}

fn summary_value(
    table: &dyn Table,
    value_column: &str,
    row: usize,
    reducer: SummaryReducer,
) -> Option<f64> {
    match reducer {
        SummaryReducer::Count => {
            table.value(value_column, row).and_then(
                |value| {
                    if value.is_null() {
                        None
                    } else {
                        Some(1.0)
                    }
                },
            )
        }
        _ => cell_f64(table, value_column, row),
    }
}

fn group_key(table: &dyn Table, by_columns: &[&str], row: usize) -> Option<Vec<DataValue>> {
    let mut key = Vec::with_capacity(by_columns.len());
    for column in by_columns {
        match table.value(column, row)? {
            DataValueRef::Null | DataValueRef::Geometry(_) => return None,
            value => key.push(value.to_owned()),
        }
    }
    Some(key)
}

fn group_key_domain(
    table: &dyn Table,
    by_columns: &[&str],
    x_column: &str,
    value_column: Option<&str>,
) -> Vec<Vec<DataValue>> {
    if by_columns.is_empty() {
        return vec![Vec::new()];
    }
    let mut keys = BTreeMap::new();
    for row in 0..table.row_count() {
        if cell_f64(table, x_column, row).is_none()
            || value_column.is_some_and(|column| table.value(column, row).is_none())
        {
            continue;
        }
        let Some(key) = group_key(table, by_columns, row) else {
            continue;
        };
        keys.insert(key, ());
    }
    keys.into_keys().collect()
}

fn push_group_key(key: &[DataValue], builders: &mut [ColumnBuilder]) {
    for (value, builder) in key.iter().cloned().zip(builders.iter_mut()) {
        builder.push_value(Some(value));
    }
}

fn push_measure(values: &[f64], reducer: SummaryReducer, builders: &mut [ColumnBuilder]) {
    let value = reduce(values, reducer);
    let count = values.len() as i64;
    builders[0].push_value(value.map(DataValue::Float));
    builders[1].push_value(Some(DataValue::Int(count)));
    if reducer == SummaryReducer::MeanSe {
        let (lower, upper, se) = mean_se(values);
        builders[2].push_value(lower.map(DataValue::Float));
        builders[3].push_value(upper.map(DataValue::Float));
        builders[4].push_value(se.map(DataValue::Float));
    }
}

fn reduce(values: &[f64], reducer: SummaryReducer) -> Option<f64> {
    match reducer {
        SummaryReducer::Count => Some(values.len() as f64),
        _ if values.is_empty() => None,
        SummaryReducer::Mean | SummaryReducer::MeanSe => {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
        SummaryReducer::Min => values.iter().copied().reduce(f64::min),
        SummaryReducer::Max => values.iter().copied().reduce(f64::max),
        SummaryReducer::Sum => Some(values.iter().sum()),
        SummaryReducer::Median => {
            let mut sorted = values.to_vec();
            sorted.sort_by(f64::total_cmp);
            Some(percentile(&sorted, 0.5))
        }
    }
}

fn mean_se(values: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    if values.is_empty() {
        return (None, None, None);
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let se = if values.len() > 1 {
        let variance = values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64;
        variance.sqrt() / (values.len() as f64).sqrt()
    } else {
        0.0
    };
    (Some(mean - se), Some(mean + se), Some(se))
}

fn summary_schema(table: &dyn Table, by_columns: &[&str], has_bounds: bool) -> Vec<ColumnDef> {
    let mut schema = group_schema(table, by_columns);
    schema.extend(measure_schema(has_bounds));
    schema
}

fn group_schema(table: &dyn Table, by_columns: &[&str]) -> Vec<ColumnDef> {
    by_columns
        .iter()
        .map(|name| {
            table
                .schema()
                .iter()
                .find(|column| column.name == *name)
                .cloned()
                .unwrap_or_else(|| col_def(name, DataType::String))
        })
        .collect()
}

fn measure_schema(has_bounds: bool) -> Vec<ColumnDef> {
    let mut schema = vec![
        col_def("value", DataType::Float),
        col_def("count", DataType::Integer),
    ];
    if has_bounds {
        schema.extend([
            col_def("lower", DataType::Float),
            col_def("upper", DataType::Float),
            col_def("se", DataType::Float),
        ]);
    }
    schema
}

fn cut_index(value: f64, breaks: &[f64]) -> Option<usize> {
    if breaks.is_empty() || value < breaks[0] {
        return None;
    }
    for index in 0..breaks.len() {
        let lo = breaks[index];
        let hi = breaks.get(index + 1).copied();
        if value >= lo && hi.map_or(true, |hi| value < hi) {
            return Some(index);
        }
    }
    None
}

fn cut_label(index: usize, breaks: &[f64], labels: Option<&[String]>) -> String {
    if let Some(label) = labels.and_then(|labels| labels.get(index)) {
        return label.clone();
    }
    match breaks.get(index + 1) {
        Some(hi) => format!("{}-{}", num(breaks[index]), num(*hi)),
        None => format!("{}+", num(breaks[index])),
    }
}

fn qq_schema() -> Vec<ColumnDef> {
    vec![
        col_def("theoretical", DataType::Float),
        col_def("sample", DataType::Float),
        col_def("line_x", DataType::Float),
        col_def("line_y", DataType::Float),
        col_def("line_xend", DataType::Float),
        col_def("line_yend", DataType::Float),
        col_def("role", DataType::String),
    ]
}

fn empty_qq_frame(schema: Vec<ColumnDef>) -> DataFrame {
    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(Vec::new()),
            Column::from_float_options(Vec::new()),
            Column::from_float_options(Vec::new()),
            Column::from_float_options(Vec::new()),
            Column::from_float_options(Vec::new()),
            Column::from_float_options(Vec::new()),
            Column::String(Vec::new()),
        ],
    )
}

/// Acklam's inverse-normal CDF approximation. It is deterministic, dependency-
/// free, and accurate enough for QQ plot placement.
fn normal_quantile(p: f64) -> f64 {
    let p = p.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.024_25;
    const P_HIGH: f64 = 1.0 - P_LOW;
    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        return (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    if p > P_HIGH {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        return -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    let q = p - 0.5;
    let r = q * q;
    (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
        / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
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
                Some(DataValue::Float(value)) if value.is_finite() => Some(value.round() as i64),
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
