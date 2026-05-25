//! Schema-only planning helpers for built-in stats.
//!
//! These functions describe the columns a stat will produce from an already
//! typed input frame. They do not inspect row values or execute transforms, so
//! the analyzer and LSP can make derived-table schemas available before the
//! renderer materializes any data (spec §10.6, §24.2).

use algraf_data::DataType;

use crate::ir::{ColumnDefIr, ColumnRef, FrameIr, StatKind};

/// Return the output schema for a built-in stat from its typed input frame.
pub fn stat_output_schema(kind: StatKind, input: &FrameIr) -> Vec<ColumnDefIr> {
    match kind {
        StatKind::Bin => match input {
            FrameIr::Vector(column) => bin_output_schema(column.dtype),
            _ => bin_output_schema(DataType::Float),
        },
        StatKind::Bin2D => bin2d_output_schema(),
        StatKind::HexBin => hexbin_output_schema(),
        StatKind::Smooth => smooth_output_schema(),
        StatKind::Density => density_output_schema(),
        StatKind::Count => count_output_schema(&frame_group_columns(input)),
        StatKind::Boxplot => Vec::new(),
    }
}

/// Best-effort output names for dependency planning before input types are
/// known. Unknown stat names return no producers.
pub(crate) fn stat_output_names_for_source(stat_name: &str) -> Vec<String> {
    match stat_name {
        "Bin" => bin_output_schema(DataType::Float),
        "Smooth" => smooth_output_schema(),
        "Bin2D" => bin2d_output_schema(),
        "HexBin" => hexbin_output_schema(),
        _ => Vec::new(),
    }
    .into_iter()
    .map(|column| column.name)
    .collect()
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

/// Boundary columns stay temporal only when the source column is temporal.
pub fn bin_boundary_dtype(input_dtype: DataType) -> DataType {
    if input_dtype == DataType::Temporal {
        DataType::Temporal
    } else {
        DataType::Float
    }
}

/// Output schema for linear smoothing.
pub fn smooth_output_schema() -> Vec<ColumnDefIr> {
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
