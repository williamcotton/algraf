use algraf_data::{Column, DataFrame, DataType, Table};

use crate::scale::{categorical_domain, cell_category};

use super::util::{col_def, deterministic_frame};

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

    let mut rows: Vec<(String, Option<String>, i64)> = Vec::new();
    if let Some(inner_col) = inner {
        let inner_cats = categorical_domain(table, inner_col);
        for o in &outer_cats {
            for i in &inner_cats {
                let count: i64 = (0..table.row_count())
                    .filter(|&row| {
                        cell_category(table, outer, row).as_deref() == Some(o.as_str())
                            && cell_category(table, inner_col, row).as_deref() == Some(i.as_str())
                    })
                    .count() as i64;
                rows.push((o.clone(), Some(i.clone()), count));
            }
        }
    } else {
        for o in &outer_cats {
            let count: i64 = (0..table.row_count())
                .filter(|&row| cell_category(table, outer, row).as_deref() == Some(o.as_str()))
                .count() as i64;
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
    columns.push(Column::Int(rows.iter().map(|r| Some(r.2)).collect()));
    deterministic_frame(schema, columns)
}
