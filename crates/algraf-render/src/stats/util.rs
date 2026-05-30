use algraf_data::{Column, ColumnDef, DataFrame, DataType};

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
