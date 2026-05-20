//! The homegrown columnar dataframe (spec §10.5).
//!
//! Storage is column-oriented. The [`Table`] trait is the only access surface
//! other crates depend on, so a future Polars backend can implement it without
//! changing language or renderer interfaces (spec §10.5).

use indexmap::IndexMap;

use crate::schema::ColumnDef;
use crate::value::{DataValueRef, DateTimeValue};

/// Typed column storage. Missing cells are `None`.
#[derive(Debug, Clone)]
pub enum Column {
    Bool(Vec<Option<bool>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    Temporal(Vec<Option<DateTimeValue>>),
    /// Backing store for `String`, `Mixed`, and `Unknown` columns: raw values
    /// are preserved where typed inference does not apply (spec §10.2, §10.3).
    String(Vec<Option<String>>),
}

impl Column {
    pub fn len(&self) -> usize {
        match self {
            Column::Bool(v) => v.len(),
            Column::Int(v) => v.len(),
            Column::Float(v) => v.len(),
            Column::Temporal(v) => v.len(),
            Column::String(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the value at `row`, or `None` if the row is out of range. A
    /// present-but-missing cell returns `Some(DataValueRef::Null)`.
    pub fn get(&self, row: usize) -> Option<DataValueRef<'_>> {
        match self {
            Column::Bool(v) => v.get(row).map(|c| opt(*c, DataValueRef::Bool)),
            Column::Int(v) => v.get(row).map(|c| opt(*c, DataValueRef::Int)),
            Column::Float(v) => v.get(row).map(|c| opt(*c, DataValueRef::Float)),
            Column::Temporal(v) => v.get(row).map(|c| opt(*c, DataValueRef::Temporal)),
            Column::String(v) => v.get(row).map(|c| match c {
                Some(s) => DataValueRef::String(s),
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

/// Read-only tabular access (spec §10.5). The only data surface exposed to
/// parser, semantics, LSP, and renderer crates.
pub trait Table {
    fn schema(&self) -> &[ColumnDef];
    fn row_count(&self) -> usize;
    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>>;
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
