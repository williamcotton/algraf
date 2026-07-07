use algraf_data::geo_types::Geometry;
use algraf_data::{
    Column, ColumnDef, DataFrame, DataType, DataValue, DataValueRef, DateTimeValue, Table,
};

use crate::svg::num;

/// Finish a stat output after its rows have been emitted in stable order.
///
/// All public stat constructors return through this helper so determinism is a
/// visible module-boundary contract (spec §18.12). Callers must build columns in
/// an order that depends only on trained domains or sorted keys, never on input
/// row order.
pub(crate) fn deterministic_frame(schema: Vec<ColumnDef>, columns: Vec<Column>) -> DataFrame {
    DataFrame::new(schema, columns)
}

pub(crate) fn col_def(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntCoercion {
    Strict,
    RoundFiniteFloats,
}

pub(crate) fn builders_for_schema(
    schema: &[ColumnDef],
    int_coercion: IntCoercion,
) -> Vec<ColumnBuilder> {
    schema
        .iter()
        .map(|column| ColumnBuilder::new(column.dtype, int_coercion))
        .collect()
}

pub(crate) fn push_passthrough(
    table: &dyn Table,
    row: usize,
    schema: &[ColumnDef],
    builders: &mut [ColumnBuilder],
) {
    for (column, builder) in schema.iter().zip(builders.iter_mut()) {
        builder.push_ref(table.value(&column.name, row));
    }
}

pub(crate) fn finish_builders(builders: Vec<ColumnBuilder>) -> Vec<Column> {
    builders.into_iter().map(ColumnBuilder::finish).collect()
}

pub(crate) fn empty_frame(schema: Vec<ColumnDef>) -> DataFrame {
    let columns = finish_builders(builders_for_schema(&schema, IntCoercion::Strict));
    deterministic_frame(schema, columns)
}

pub(crate) enum ColumnBuilder {
    Bool(Vec<Option<bool>>),
    Int {
        values: Vec<Option<i64>>,
        int_coercion: IntCoercion,
    },
    Float(Vec<Option<f64>>),
    Temporal(Vec<Option<DateTimeValue>>),
    String(Vec<Option<String>>),
    Geometry(Vec<Option<Geometry<f64>>>),
}

impl ColumnBuilder {
    pub(crate) fn new(dtype: DataType, int_coercion: IntCoercion) -> Self {
        match dtype {
            DataType::Boolean => ColumnBuilder::Bool(Vec::new()),
            DataType::Integer => ColumnBuilder::Int {
                values: Vec::new(),
                int_coercion,
            },
            DataType::Float => ColumnBuilder::Float(Vec::new()),
            DataType::Temporal => ColumnBuilder::Temporal(Vec::new()),
            DataType::Geometry => ColumnBuilder::Geometry(Vec::new()),
            DataType::String | DataType::Mixed | DataType::Unknown => {
                ColumnBuilder::String(Vec::new())
            }
        }
    }

    pub(crate) fn push_ref(&mut self, value: Option<DataValueRef<'_>>) {
        let value = value.and_then(|value| match value {
            DataValueRef::Null => None,
            value => Some(value.to_owned()),
        });
        self.push_value(value);
    }

    pub(crate) fn push_value(&mut self, value: Option<DataValue>) {
        match self {
            ColumnBuilder::Bool(values) => values.push(match value {
                Some(DataValue::Bool(value)) => Some(value),
                _ => None,
            }),
            ColumnBuilder::Int {
                values,
                int_coercion,
            } => values.push(match value {
                Some(DataValue::Int(value)) => Some(value),
                Some(DataValue::Float(value))
                    if *int_coercion == IntCoercion::RoundFiniteFloats && value.is_finite() =>
                {
                    Some(value.round() as i64)
                }
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

    pub(crate) fn push_null(&mut self) {
        self.push_value(None);
    }

    pub(crate) fn finish(self) -> Column {
        match self {
            ColumnBuilder::Bool(values) => Column::from_bool_options(values),
            ColumnBuilder::Int { values, .. } => Column::from_int_options(values),
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

#[cfg(test)]
mod tests {
    use algraf_data::{DataValueRef, TemporalPrecision};
    use chrono::NaiveDate;
    use geo_types::Point;

    use super::*;

    #[test]
    fn integer_builder_obeys_coercion_policy() {
        let mut strict = ColumnBuilder::new(DataType::Integer, IntCoercion::Strict);
        strict.push_value(Some(DataValue::Float(2.6)));
        strict.push_value(Some(DataValue::Int(4)));
        let strict = strict.finish();
        assert_eq!(strict.get(0), Some(DataValueRef::Null));
        assert_eq!(strict.get(1), Some(DataValueRef::Int(4)));

        let mut rounded = ColumnBuilder::new(DataType::Integer, IntCoercion::RoundFiniteFloats);
        rounded.push_value(Some(DataValue::Float(2.6)));
        rounded.push_value(Some(DataValue::Float(f64::INFINITY)));
        let rounded = rounded.finish();
        assert_eq!(rounded.get(0), Some(DataValueRef::Int(3)));
        assert_eq!(rounded.get(1), Some(DataValueRef::Null));
    }

    #[test]
    fn stat_string_conversion_is_locale_independent() {
        let temporal = DateTimeValue::new(
            NaiveDate::from_ymd_opt(2024, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            TemporalPrecision::DateTime,
        );

        assert_eq!(
            value_to_string(DataValue::Bool(true)),
            Some("true".to_string())
        );
        assert_eq!(value_to_string(DataValue::Int(42)), Some("42".to_string()));
        assert_eq!(
            value_to_string(DataValue::Float(1.25)),
            Some("1.25".to_string())
        );
        assert_eq!(
            value_to_string(DataValue::Temporal(temporal)),
            Some("2024-01-02T03:04:05+00:00".to_string())
        );
        assert_eq!(
            value_to_string(DataValue::Geometry(Geometry::Point(Point::new(1.0, 2.0)))),
            None
        );
    }
}
