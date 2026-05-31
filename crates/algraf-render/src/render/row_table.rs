use algraf_data::{ColumnDef, DataValueRef, Table};

pub(super) struct RowSubsetTable<'t, 'r> {
    table: &'t dyn Table,
    rows: &'r [usize],
}

impl<'t, 'r> RowSubsetTable<'t, 'r> {
    pub(super) fn new(table: &'t dyn Table, rows: &'r [usize]) -> Self {
        RowSubsetTable { table, rows }
    }
}

impl Table for RowSubsetTable<'_, '_> {
    fn schema(&self) -> &[ColumnDef] {
        self.table.schema()
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>> {
        let source_row = *self.rows.get(row)?;
        self.table.value(column, source_row)
    }

    fn column(&self, _column: &str) -> Option<algraf_data::ColumnView<'_>> {
        None
    }
}
