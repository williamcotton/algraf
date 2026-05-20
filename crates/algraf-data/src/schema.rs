//! Column definitions and data types (spec §10.4).

/// The inferred type of a column (spec §10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Boolean,
    Integer,
    Float,
    Temporal,
    String,
    /// A mix of incompatible types; treated as categorical (spec §10.3).
    Mixed,
    /// No non-missing values were observed.
    Unknown,
}

impl DataType {
    /// Whether a continuous scale can be trained from this type.
    pub fn is_continuous(self) -> bool {
        matches!(self, DataType::Integer | DataType::Float)
    }

    /// Whether this type is naturally categorical.
    pub fn is_categorical(self) -> bool {
        matches!(self, DataType::Boolean | DataType::String | DataType::Mixed)
    }
}

/// A column's name, inferred type, nullability, and sample values (spec §10.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub name: String,
    pub dtype: DataType,
    pub nullable: bool,
    /// A few example raw values, used for LSP hover (deterministic order).
    pub examples: Vec<String>,
}
