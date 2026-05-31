//! The homegrown columnar dataframe (spec §10.5).
//!
//! Storage is column-oriented. The [`Table`] trait is the only access surface
//! other crates depend on, so a future Polars backend can implement it without
//! changing language or renderer interfaces (spec §10.5).

use geo_types::Geometry;
use indexmap::IndexMap;

use crate::schema::ColumnDef;
use crate::value::{DataValueRef, DateTimeValue};

/// Bit-level validity mask for nullable scalar columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullBitmap {
    len: usize,
    words: Vec<u64>,
}

impl NullBitmap {
    pub fn all_valid(len: usize) -> Self {
        let words = len.div_ceil(64);
        let mut bitmap = NullBitmap {
            len,
            words: vec![u64::MAX; words],
        };
        bitmap.clear_unused_bits();
        bitmap
    }

    pub fn from_bools(valid: impl IntoIterator<Item = bool>) -> Self {
        let values: Vec<bool> = valid.into_iter().collect();
        let mut bitmap = NullBitmap {
            len: values.len(),
            words: vec![0; values.len().div_ceil(64)],
        };
        for (idx, is_valid) in values.into_iter().enumerate() {
            if is_valid {
                bitmap.set_valid(idx);
            }
        }
        bitmap
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_valid(&self, row: usize) -> bool {
        if row >= self.len {
            return false;
        }
        let word = row / 64;
        let bit = row % 64;
        (self.words[word] & (1u64 << bit)) != 0
    }

    fn set_valid(&mut self, row: usize) {
        let word = row / 64;
        let bit = row % 64;
        self.words[word] |= 1u64 << bit;
    }

    fn clear_unused_bits(&mut self) {
        let Some(last) = self.words.last_mut() else {
            return;
        };
        let used = self.len % 64;
        if used != 0 {
            *last &= (1u64 << used) - 1;
        }
    }
}

/// Dense scalar values plus a bit-level validity mask.
#[derive(Debug, Clone, PartialEq)]
pub struct NullableColumn<T> {
    values: Vec<T>,
    validity: NullBitmap,
}

impl<T: Copy> NullableColumn<T> {
    pub fn from_options_with_null(values: Vec<Option<T>>, null_value: T) -> Self {
        let validity = NullBitmap::from_bools(values.iter().map(Option::is_some));
        let values = values
            .into_iter()
            .map(|value| value.unwrap_or(null_value))
            .collect();
        NullableColumn { values, validity }
    }

    pub fn from_values(values: Vec<T>) -> Self {
        let len = values.len();
        NullableColumn {
            values,
            validity: NullBitmap::all_valid(len),
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, row: usize) -> Option<Option<T>> {
        let value = *self.values.get(row)?;
        Some(self.validity.is_valid(row).then_some(value))
    }

    pub fn present_values(&self) -> impl Iterator<Item = T> + '_ {
        self.values
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(idx, value)| self.validity.is_valid(idx).then_some(value))
    }

    pub fn iter_options(&self) -> impl Iterator<Item = Option<T>> + '_ {
        self.values
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, value)| self.validity.is_valid(idx).then_some(value))
    }

    pub fn validity(&self) -> &NullBitmap {
        &self.validity
    }
}

/// Typed column storage.
#[derive(Debug, Clone)]
pub enum Column {
    Bool(NullableColumn<bool>),
    Int(NullableColumn<i64>),
    Float(NullableColumn<f64>),
    Temporal(NullableColumn<DateTimeValue>),
    /// Backing store for `String`, `Mixed`, and `Unknown` columns: raw values
    /// are preserved where typed inference does not apply (spec §10.2, §10.3).
    String(Vec<Option<String>>),
    /// Spatial geometry values, one per feature row (spec §10.11). Columnar
    /// behind the [`Table`] boundary like every other type, so parser,
    /// semantics, LSP, and render see geometry only through [`DataValueRef`].
    Geometry(Vec<Option<Geometry<f64>>>),
}

impl Column {
    pub fn from_bool_options(values: Vec<Option<bool>>) -> Self {
        Column::Bool(NullableColumn::from_options_with_null(values, false))
    }

    pub fn from_int_options(values: Vec<Option<i64>>) -> Self {
        Column::Int(NullableColumn::from_options_with_null(values, 0))
    }

    pub fn from_float_options(values: Vec<Option<f64>>) -> Self {
        Column::Float(NullableColumn::from_options_with_null(values, 0.0))
    }

    pub fn from_temporal_options(values: Vec<Option<DateTimeValue>>) -> Self {
        Column::Temporal(NullableColumn::from_options_with_null(
            values,
            DateTimeValue::unix_epoch(),
        ))
    }

    pub fn len(&self) -> usize {
        match self {
            Column::Bool(v) => v.len(),
            Column::Int(v) => v.len(),
            Column::Float(v) => v.len(),
            Column::Temporal(v) => v.len(),
            Column::String(v) => v.len(),
            Column::Geometry(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the value at `row`, or `None` if the row is out of range. A
    /// present-but-missing cell returns `Some(DataValueRef::Null)`.
    pub fn get(&self, row: usize) -> Option<DataValueRef<'_>> {
        match self {
            Column::Bool(v) => v.get(row).map(|c| opt(c, DataValueRef::Bool)),
            Column::Int(v) => v.get(row).map(|c| opt(c, DataValueRef::Int)),
            Column::Float(v) => v.get(row).map(|c| opt(c, DataValueRef::Float)),
            Column::Temporal(v) => v.get(row).map(|c| opt(c, DataValueRef::Temporal)),
            Column::String(v) => v.get(row).map(|c| match c {
                Some(s) => DataValueRef::String(s),
                None => DataValueRef::Null,
            }),
            Column::Geometry(v) => v.get(row).map(|c| match c {
                Some(g) => DataValueRef::Geometry(g),
                None => DataValueRef::Null,
            }),
        }
    }
}

fn opt<T>(cell: Option<T>, wrap: impl Fn(T) -> DataValueRef<'static>) -> DataValueRef<'static> {
    match cell {
        Some(value) => wrap(value),
        None => DataValueRef::Null,
    }
}

/// Borrowed typed column view for execution-oriented scans.
#[derive(Debug, Clone, Copy)]
pub enum ColumnView<'a> {
    Bool(&'a NullableColumn<bool>),
    Int(&'a NullableColumn<i64>),
    Float(&'a NullableColumn<f64>),
    Temporal(&'a NullableColumn<DateTimeValue>),
    String(&'a [Option<String>]),
    Geometry(&'a [Option<Geometry<f64>>]),
}

impl<'a> ColumnView<'a> {
    pub fn len(&self) -> usize {
        match self {
            ColumnView::Bool(v) => v.len(),
            ColumnView::Int(v) => v.len(),
            ColumnView::Float(v) => v.len(),
            ColumnView::Temporal(v) => v.len(),
            ColumnView::String(v) => v.len(),
            ColumnView::Geometry(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, row: usize) -> Option<DataValueRef<'a>> {
        match self {
            ColumnView::Bool(v) => v.get(row).map(|c| opt(c, DataValueRef::Bool)),
            ColumnView::Int(v) => v.get(row).map(|c| opt(c, DataValueRef::Int)),
            ColumnView::Float(v) => v.get(row).map(|c| opt(c, DataValueRef::Float)),
            ColumnView::Temporal(v) => v.get(row).map(|c| opt(c, DataValueRef::Temporal)),
            ColumnView::String(v) => v.get(row).map(|c| match c {
                Some(s) => DataValueRef::String(s),
                None => DataValueRef::Null,
            }),
            ColumnView::Geometry(v) => v.get(row).map(|c| match c {
                Some(g) => DataValueRef::Geometry(g),
                None => DataValueRef::Null,
            }),
        }
    }

    pub fn f64_at(&self, row: usize) -> Option<f64> {
        match self {
            ColumnView::Int(v) => v.get(row).flatten().map(|value| value as f64),
            ColumnView::Float(v) => v.get(row).flatten().filter(|value| value.is_finite()),
            _ => None,
        }
    }

    pub fn temporal_at(&self, row: usize) -> Option<DateTimeValue> {
        match self {
            ColumnView::Temporal(v) => v.get(row).flatten(),
            _ => None,
        }
    }

    pub fn category_at(&self, row: usize) -> Option<String> {
        match self.get(row)? {
            DataValueRef::Null => None,
            DataValueRef::Bool(b) => Some(b.to_string()),
            DataValueRef::Int(i) => Some(i.to_string()),
            DataValueRef::Float(f) => Some(crate::value::format_f64_category(f)),
            DataValueRef::Temporal(t) => Some(t.instant.and_utc().to_rfc3339()),
            DataValueRef::String(s) => Some(s.to_string()),
            DataValueRef::Geometry(_) => None,
        }
    }
}

impl<'a> From<&'a Column> for ColumnView<'a> {
    fn from(column: &'a Column) -> Self {
        match column {
            Column::Bool(v) => ColumnView::Bool(v),
            Column::Int(v) => ColumnView::Int(v),
            Column::Float(v) => ColumnView::Float(v),
            Column::Temporal(v) => ColumnView::Temporal(v),
            Column::String(v) => ColumnView::String(v),
            Column::Geometry(v) => ColumnView::Geometry(v),
        }
    }
}

/// Visitor used by [`Table::scan`].
pub trait TableScan {
    fn visit(&mut self, row: usize, values: &[Option<DataValueRef<'_>>]);
}

/// Read-only tabular access (spec §10.5). The only data surface exposed to
/// parser, semantics, LSP, and renderer crates.
pub trait Table {
    fn schema(&self) -> &[ColumnDef];
    fn row_count(&self) -> usize;
    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>>;
    fn column(&self, column: &str) -> Option<ColumnView<'_>>;

    fn scan(&self, columns: &[&str], visitor: &mut dyn TableScan) {
        for row in 0..self.row_count() {
            let values: Vec<_> = columns
                .iter()
                .map(|column| self.value(column, row))
                .collect();
            visitor.visit(row, &values);
        }
    }
}

/// An in-memory columnar table.
#[derive(Debug, Clone)]
pub struct DataFrame {
    schema: Vec<ColumnDef>,
    columns: Vec<Column>,
    name_to_index: IndexMap<String, usize>,
    row_count: usize,
}

impl DataFrame {
    /// Build a dataframe from parallel schema and column vectors.
    ///
    /// Panics only on internal misuse: mismatched lengths between `schema` and
    /// `columns`, which never happens for inferred data.
    pub fn new(schema: Vec<ColumnDef>, columns: Vec<Column>) -> DataFrame {
        assert_eq!(schema.len(), columns.len(), "schema/column count mismatch");
        let row_count = columns.first().map_or(0, Column::len);
        let mut name_to_index = IndexMap::with_capacity(schema.len());
        for (index, def) in schema.iter().enumerate() {
            name_to_index.insert(def.name.clone(), index);
        }
        DataFrame {
            schema,
            columns,
            name_to_index,
            row_count,
        }
    }

    /// The column names in canonical order.
    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.schema.iter().map(|c| c.name.as_str())
    }

    /// The definition of a column by name.
    pub fn column_def(&self, name: &str) -> Option<&ColumnDef> {
        self.name_to_index.get(name).map(|&i| &self.schema[i])
    }

    /// The raw column storage by name.
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.name_to_index.get(name).map(|&i| &self.columns[i])
    }

    /// A lightweight view of one row.
    pub fn row(&self, index: usize) -> Option<RowView<'_>> {
        (index < self.row_count).then_some(RowView { frame: self, index })
    }
}

impl Table for DataFrame {
    fn schema(&self) -> &[ColumnDef] {
        &self.schema
    }

    fn row_count(&self) -> usize {
        self.row_count
    }

    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>> {
        let index = *self.name_to_index.get(column)?;
        self.columns[index].get(row)
    }

    fn column(&self, column: &str) -> Option<ColumnView<'_>> {
        let index = *self.name_to_index.get(column)?;
        Some(ColumnView::from(&self.columns[index]))
    }

    fn scan(&self, columns: &[&str], visitor: &mut dyn TableScan) {
        let views: Vec<Option<ColumnView<'_>>> = columns
            .iter()
            .map(|column| <DataFrame as Table>::column(self, column))
            .collect();
        for row in 0..self.row_count {
            let values: Vec<_> = views
                .iter()
                .map(|view| view.and_then(|view| view.get(row)))
                .collect();
            visitor.visit(row, &values);
        }
    }
}

/// A borrowed view of a single row (spec §10.5).
#[derive(Debug, Clone, Copy)]
pub struct RowView<'a> {
    frame: &'a DataFrame,
    index: usize,
}

impl<'a> RowView<'a> {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn get(&self, column: &str) -> Option<DataValueRef<'a>> {
        self.frame.value(column, self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::TemporalPrecision;
    use chrono::NaiveDate;
    use std::mem;

    #[test]
    fn nullable_column_preserves_missing_semantics() {
        let column = Column::from_int_options(vec![Some(10), None, Some(30)]);

        assert_eq!(column.get(0), Some(DataValueRef::Int(10)));
        assert_eq!(column.get(1), Some(DataValueRef::Null));
        assert_eq!(column.get(2), Some(DataValueRef::Int(30)));
        assert_eq!(column.get(3), None);
    }

    #[test]
    fn scalar_nullable_storage_uses_dense_values_plus_bitmap() {
        let sparse: Vec<_> = (0..1024)
            .map(|i| (i % 3 != 0).then_some(i as f64))
            .collect();
        let column = NullableColumn::from_options_with_null(sparse, 0.0);

        assert_eq!(column.values.len(), 1024);
        assert_eq!(column.validity.len(), 1024);
        assert_eq!(
            mem::size_of_val(column.values.as_slice()),
            1024 * mem::size_of::<f64>()
        );
        assert!(mem::size_of_val(column.validity.words.as_slice()) < 1024 * mem::size_of::<bool>());
    }

    #[test]
    fn temporal_null_sentinel_is_never_observable() {
        let observed = DateTimeValue::new(
            NaiveDate::from_ymd_opt(2024, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            TemporalPrecision::DateTime,
        );
        let column = Column::from_temporal_options(vec![None, Some(observed)]);

        assert_eq!(column.get(0), Some(DataValueRef::Null));
        assert_eq!(column.get(1), Some(DataValueRef::Temporal(observed)));
    }
}
