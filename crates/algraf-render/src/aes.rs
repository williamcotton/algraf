//! Aesthetic resolution: turning geometry mappings and settings into per-row
//! colors, opacity, and size (spec §16.8).

use algraf_data::{DataType, Table};
use algraf_semantics::{GeometryIr, ScaleIr, ScaleTargetIr, SettingValue};

use crate::scale::{categorical_domain, cell_category, cell_f64, numeric_domain};
use crate::svg::num;
use crate::theme::{categorical_color_from, gradient_color_from, CONTINUOUS_GRADIENT};

/// How an aesthetic resolves to a color.
#[derive(Debug, Clone)]
pub enum ColorSpec {
    None,
    Constant(String),
    Categorical {
        col: String,
        categories: Vec<String>,
        palette: Option<String>,
    },
    Gradient {
        col: String,
        min: f64,
        max: f64,
        stops: Vec<String>,
    },
}

impl ColorSpec {
    /// The color for a row, if resolvable.
    pub fn resolve(&self, table: &dyn Table, row: usize) -> Option<String> {
        match self {
            ColorSpec::None => None,
            ColorSpec::Constant(c) => Some(c.clone()),
            ColorSpec::Categorical {
                col,
                categories,
                palette,
            } => {
                let cat = cell_category(table, col, row)?;
                let index = categories.iter().position(|c| *c == cat)?;
                Some(categorical_color_from(palette.as_deref(), index).to_string())
            }
            ColorSpec::Gradient {
                col,
                min,
                max,
                stops,
            } => {
                let v = cell_f64(table, col, row)?;
                let t = if (max - min).abs() < f64::EPSILON {
                    0.5
                } else {
                    (v - min) / (max - min)
                };
                Some(gradient_at(stops, t))
            }
        }
    }

    /// A legend for this aesthetic, if it is a data mapping (spec §19.5).
    pub fn legend(&self, title: &str) -> Option<Legend> {
        match self {
            ColorSpec::Categorical {
                categories,
                palette,
                ..
            } => Some(Legend {
                title: title.to_string(),
                kind: LegendKind::Discrete,
                entries: categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        (
                            c.clone(),
                            categorical_color_from(palette.as_deref(), i).to_string(),
                        )
                    })
                    .collect(),
                stroke_entries: Vec::new(),
            }),
            ColorSpec::Gradient {
                min, max, stops, ..
            } => {
                let ticks = gradient_legend_ticks(*min, *max);
                Some(Legend {
                    title: title.to_string(),
                    kind: LegendKind::Continuous,
                    stroke_entries: Vec::new(),
                    entries: ticks
                        .into_iter()
                        .map(|value| {
                            let t = if (max - min).abs() < f64::EPSILON {
                                0.5
                            } else {
                                (value - min) / (max - min)
                            };
                            (num(value), gradient_at(stops, t))
                        })
                        .collect(),
                })
            }
            _ => None,
        }
    }
}

fn gradient_legend_ticks(min: f64, max: f64) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    if (max - min).abs() < f64::EPSILON {
        vec![min]
    } else {
        (0..=4)
            .map(|i| min + (max - min) * f64::from(i) / 4.0)
            .collect()
    }
}

/// A legend model (spec §19.5).
///
/// `entries` holds `(label, fill_color)` swatches. `stroke_entries`, when
/// non-empty, is aligned with `entries` and supplies a per-entry stroke color
/// for the swatch; it is populated when a `fill` and `stroke` legend over the
/// same categorical column are merged into one (spec §19.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Legend {
    pub title: String,
    pub kind: LegendKind,
    pub entries: Vec<(String, String)>,
    pub stroke_entries: Vec<String>,
}

/// How a legend's entries should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendKind {
    Discrete,
    Continuous,
}

/// Build a color specification for an aesthetic (`"fill"` or `"stroke"`).
pub fn color_spec(
    geo: &GeometryIr,
    aesthetic: &str,
    table: &dyn Table,
    scales: &[ScaleIr],
) -> ColorSpec {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
        let col = &mapping.column.name;
        return match mapping.column.dtype {
            DataType::Integer | DataType::Float => {
                let (min, max) = numeric_domain(table, col).unwrap_or((0.0, 1.0));
                ColorSpec::Gradient {
                    col: col.clone(),
                    min,
                    max,
                    stops: gradient_for(scales, aesthetic, col).unwrap_or_else(default_gradient),
                }
            }
            _ => ColorSpec::Categorical {
                col: col.clone(),
                categories: categorical_domain(table, col),
                palette: palette_for(scales, aesthetic, col),
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

fn gradient_at(stops: &[String], t: f64) -> String {
    let borrowed: Vec<&str> = stops.iter().map(String::as_str).collect();
    gradient_color_from(&borrowed, t)
}

fn default_gradient() -> Vec<String> {
    CONTINUOUS_GRADIENT
        .iter()
        .map(|stop| (*stop).to_string())
        .collect()
}

fn gradient_for(scales: &[ScaleIr], aesthetic: &str, column: &str) -> Option<Vec<String>> {
    scales.iter().rev().find_map(|scale| match &scale.target {
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: Some(scale_column),
        } if target == aesthetic && scale_column.name == column => scale.gradient.clone(),
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: None,
        } if target == aesthetic => scale.gradient.clone(),
        _ => None,
    })
}

fn palette_for(scales: &[ScaleIr], aesthetic: &str, column: &str) -> Option<String> {
    scales.iter().rev().find_map(|scale| match &scale.target {
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: Some(scale_column),
        } if target == aesthetic && scale_column.name == column => scale.palette.clone(),
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: None,
        } if target == aesthetic => scale.palette.clone(),
        _ => None,
    })
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
