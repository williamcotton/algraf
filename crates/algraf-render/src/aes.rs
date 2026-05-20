//! Aesthetic resolution: turning geometry mappings and settings into per-row
//! colors, opacity, and size (spec §16.8).

use algraf_data::{DataType, Table};
use algraf_semantics::{GeometryIr, SettingValue};

use crate::scale::{categorical_domain, cell_category, cell_f64, numeric_domain};
use crate::theme::{categorical_color, gradient_color};

/// How an aesthetic resolves to a color.
#[derive(Debug, Clone)]
pub enum ColorSpec {
    None,
    Constant(String),
    Categorical {
        col: String,
        categories: Vec<String>,
    },
    Gradient {
        col: String,
        min: f64,
        max: f64,
    },
}

impl ColorSpec {
    /// The color for a row, if resolvable.
    pub fn resolve(&self, table: &dyn Table, row: usize) -> Option<String> {
        match self {
            ColorSpec::None => None,
            ColorSpec::Constant(c) => Some(c.clone()),
            ColorSpec::Categorical { col, categories } => {
                let cat = cell_category(table, col, row)?;
                let index = categories.iter().position(|c| *c == cat)?;
                Some(categorical_color(index).to_string())
            }
            ColorSpec::Gradient { col, min, max } => {
                let v = cell_f64(table, col, row)?;
                let t = if (max - min).abs() < f64::EPSILON {
                    0.5
                } else {
                    (v - min) / (max - min)
                };
                Some(gradient_color(t))
            }
        }
    }

    /// A legend for this aesthetic, if it is a categorical mapping (spec §19.5).
    pub fn legend(&self, title: &str) -> Option<Legend> {
        match self {
            ColorSpec::Categorical { categories, .. } => Some(Legend {
                title: title.to_string(),
                entries: categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.clone(), categorical_color(i).to_string()))
                    .collect(),
            }),
            _ => None,
        }
    }
}

/// A legend model (spec §19.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Legend {
    pub title: String,
    pub entries: Vec<(String, String)>,
}

/// Build a color specification for an aesthetic (`"fill"` or `"stroke"`).
pub fn color_spec(geo: &GeometryIr, aesthetic: &str, table: &dyn Table) -> ColorSpec {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
        let col = &mapping.column.name;
        return match mapping.column.dtype {
            DataType::Integer | DataType::Float => {
                let (min, max) = numeric_domain(table, col).unwrap_or((0.0, 1.0));
                ColorSpec::Gradient {
                    col: col.clone(),
                    min,
                    max,
                }
            }
            _ => ColorSpec::Categorical {
                col: col.clone(),
                categories: categorical_domain(table, col),
            },
        };
    }
    if let Some(setting) = geo.settings.iter().find(|s| s.name == aesthetic) {
        if let SettingValue::String(c) = &setting.value {
            return ColorSpec::Constant(c.clone());
        }
    }
    ColorSpec::None
}

/// A constant numeric setting value, or a default.
pub fn number_setting(geo: &GeometryIr, name: &str, default: f64) -> f64 {
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => Some(n),
            _ => None,
        })
        .unwrap_or(default)
}
