//! Aesthetic resolution: turning geometry mappings and settings into per-row
//! colors, opacity, and size (spec §16.8).

use algraf_data::{DataType, DataValueRef, Table};
use algraf_semantics::{
    GeometryIr, GradientIr, PropertyKey, ScaleIr, ScaleModeIr, ScaleTargetIr, SettingValue,
};

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
        /// Explicit per-category colors aligned with `categories`, from a manual
        /// `range: ["A" => "..."]` map (spec §16.13). `None` falls back to the
        /// palette.
        colors: Option<Vec<String>>,
        /// Explicit per-category legend labels aligned with `categories`, from a
        /// `labels: ["A" => "..."]` map (spec §16.13).
        labels: Option<Vec<String>>,
    },
    Gradient {
        col: String,
        min: f64,
        max: f64,
        stops: GradientIr,
        breaks: Option<Vec<f64>>,
        labels: Option<Vec<String>>,
    },
    Binned {
        col: String,
        breaks: Vec<f64>,
        labels: Vec<String>,
        colors: Vec<String>,
    },
    Identity {
        col: String,
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
                colors,
                ..
            } => {
                let cat = cell_category(table, col, row)?;
                let index = categories.iter().position(|c| *c == cat)?;
                if let Some(colors) = colors {
                    if let Some(color) = colors.get(index) {
                        return Some(color.clone());
                    }
                }
                Some(categorical_color_from(palette.as_deref(), index).to_string())
            }
            ColorSpec::Gradient {
                col,
                min,
                max,
                stops,
                ..
            } => {
                let v = cell_f64(table, col, row)?;
                let t = if (max - min).abs() < f64::EPSILON {
                    0.5
                } else {
                    (v - min) / (max - min)
                };
                Some(gradient_at(stops, v, *min, *max, t))
            }
            ColorSpec::Binned {
                col,
                breaks,
                colors,
                ..
            } => {
                let v = cell_f64(table, col, row)?;
                let index = bin_index_for_value(v, breaks)?;
                colors.get(index).cloned()
            }
            ColorSpec::Identity { col } => match table.value(col, row)? {
                DataValueRef::String(value) if is_safe_svg_color(value) => Some(value.to_string()),
                _ => None,
            },
        }
    }

    /// A legend for this aesthetic, if it is a data mapping (spec §19.5).
    pub fn legend(&self, title: &str) -> Option<Legend> {
        match self {
            ColorSpec::Categorical {
                categories,
                palette,
                colors,
                labels,
                ..
            } => Some(Legend {
                title: title.to_string(),
                kind: LegendKind::Discrete,
                entries: categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let label = labels
                            .as_ref()
                            .and_then(|l| l.get(i).cloned())
                            .unwrap_or_else(|| c.clone());
                        let color = colors
                            .as_ref()
                            .and_then(|c| c.get(i).cloned())
                            .unwrap_or_else(|| {
                                categorical_color_from(palette.as_deref(), i).to_string()
                            });
                        (label, color)
                    })
                    .collect(),
                stroke_entries: Vec::new(),
                sizes: Vec::new(),
                shapes: Vec::new(),
                images: Vec::new(),
            }),
            ColorSpec::Gradient {
                min,
                max,
                stops,
                breaks,
                labels,
                ..
            } => {
                let ticks = breaks
                    .clone()
                    .unwrap_or_else(|| gradient_legend_ticks(*min, *max));
                Some(Legend {
                    title: title.to_string(),
                    kind: LegendKind::Continuous,
                    stroke_entries: Vec::new(),
                    sizes: Vec::new(),
                    shapes: Vec::new(),
                    images: Vec::new(),
                    entries: ticks
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| {
                            let t = if (max - min).abs() < f64::EPSILON {
                                0.5
                            } else {
                                (value - min) / (max - min)
                            };
                            let label = labels
                                .as_ref()
                                .and_then(|labels| labels.get(index).cloned())
                                .unwrap_or_else(|| num(value));
                            (label, gradient_at(stops, value, *min, *max, t))
                        })
                        .collect(),
                })
            }
            ColorSpec::Binned { labels, colors, .. } => Some(Legend {
                title: title.to_string(),
                kind: LegendKind::Discrete,
                entries: labels.iter().cloned().zip(colors.iter().cloned()).collect(),
                stroke_entries: Vec::new(),
                sizes: Vec::new(),
                shapes: Vec::new(),
                images: Vec::new(),
            }),
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
///
/// `sizes`, when non-empty, is aligned with `entries` and holds the resolved
/// magnitude (line thickness or circle radius, in px) for each swatch of a
/// [`LegendKind::Width`] or [`LegendKind::Radius`] size legend.
///
/// `shapes`, when non-empty, is aligned with `entries` and holds the marker
/// shape for each swatch of a discrete legend whose column is also mapped to
/// `shape`; the swatch is drawn as that glyph rather than a square so the legend
/// matches the points (spec §19.5).
///
/// `images`, when non-empty, is aligned with `entries` and holds the embedded
/// raster/SVG asset for each swatch of an image legend.
#[derive(Debug, Clone, PartialEq)]
pub struct Legend {
    pub title: String,
    pub kind: LegendKind,
    pub entries: Vec<(String, String)>,
    pub stroke_entries: Vec<String>,
    pub sizes: Vec<f64>,
    pub shapes: Vec<crate::marker::MarkerShape>,
    pub images: Vec<LegendImage>,
}

/// Embedded image data used by an image legend entry.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendImage {
    pub href: String,
    pub intrinsic_width: f64,
    pub intrinsic_height: f64,
}

/// How a legend's entries should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendKind {
    Discrete,
    Image,
    Continuous,
    /// A `strokeWidth` size legend: each swatch is a line of the mapped thickness.
    Width,
    /// A `size` size legend: each swatch is a circle of the mapped radius.
    Radius,
}

/// Build a color specification for an aesthetic ([`PropertyKey::Fill`] or
/// [`PropertyKey::Stroke`]).
pub fn color_spec(
    geo: &GeometryIr,
    aesthetic: PropertyKey,
    table: &dyn Table,
    scales: &[ScaleIr],
) -> ColorSpec {
    let aesthetic_name = aesthetic.as_str();
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
        let col = &mapping.column.name;
        let scale = aesthetic_scale(scales, aesthetic_name, col);
        if scale.and_then(|scale| scale.mode) == Some(ScaleModeIr::Identity) {
            return ColorSpec::Identity { col: col.clone() };
        }
        return match mapping.column.dtype {
            DataType::Integer | DataType::Float => {
                let (data_min, data_max) = numeric_domain(table, col).unwrap_or((0.0, 1.0));
                let (min, max) = match scale.and_then(|scale| scale.domain) {
                    Some([lo, hi]) => (lo.unwrap_or(data_min), hi.unwrap_or(data_max)),
                    None => (data_min, data_max),
                };
                if scale.and_then(|scale| scale.mode) == Some(ScaleModeIr::Binned) {
                    let colors = binned_colors(scale, min, max);
                    let breaks = scale
                        .and_then(|scale| scale.breaks.clone())
                        .unwrap_or_else(|| default_binned_breaks(min, max, colors.len()));
                    let labels = scale
                        .and_then(|scale| scale.break_labels.clone())
                        .unwrap_or_else(|| default_binned_labels(&breaks));
                    ColorSpec::Binned {
                        col: col.clone(),
                        breaks,
                        labels,
                        colors,
                    }
                } else {
                    ColorSpec::Gradient {
                        col: col.clone(),
                        min,
                        max,
                        stops: gradient_for(scales, aesthetic_name, col)
                            .unwrap_or_else(default_gradient),
                        breaks: scale.and_then(|scale| scale.breaks.clone()),
                        labels: scale.and_then(|scale| scale.break_labels.clone()),
                    }
                }
            }
            _ => {
                // A manual `range: ["A" => "..."]` map fixes both the category
                // order and the colors; otherwise categories come from the data
                // in first-appearance order (spec §16.13).
                if let Some(map) = color_map_for(scales, aesthetic_name, col) {
                    let categories: Vec<String> = map.iter().map(|(k, _)| k.clone()).collect();
                    let colors: Vec<String> = map.iter().map(|(_, v)| v.clone()).collect();
                    let labels = label_map_for(scales, aesthetic_name, col).map(|lm| {
                        categories
                            .iter()
                            .map(|cat| {
                                lm.iter()
                                    .find(|(k, _)| k == cat)
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_else(|| cat.clone())
                            })
                            .collect()
                    });
                    ColorSpec::Categorical {
                        col: col.clone(),
                        categories,
                        palette: None,
                        colors: Some(colors),
                        labels,
                    }
                } else {
                    ColorSpec::Categorical {
                        col: col.clone(),
                        categories: categorical_domain(table, col),
                        palette: palette_for(scales, aesthetic_name, col),
                        colors: None,
                        labels: None,
                    }
                }
            }
        };
    }
    if let Some(setting) = geo.settings.iter().find(|s| s.name == aesthetic) {
        if let SettingValue::String(c) = &setting.value {
            return ColorSpec::Constant(c.clone());
        }
    }
    ColorSpec::None
}

fn gradient_at(stops: &GradientIr, value: f64, _min: f64, _max: f64, t: f64) -> String {
    match stops {
        GradientIr::Even(stops) => {
            let borrowed: Vec<&str> = stops.iter().map(String::as_str).collect();
            gradient_color_from(&borrowed, t)
        }
        GradientIr::Positioned(stops) => {
            if stops.is_empty() {
                return "#000000".to_string();
            }
            if value <= stops[0].value {
                return stops[0].color.clone();
            }
            if let Some(last) = stops.last() {
                if value >= last.value {
                    return last.color.clone();
                }
            }
            for pair in stops.windows(2) {
                let a = &pair[0];
                let b = &pair[1];
                if value >= a.value && value <= b.value {
                    let local = if (b.value - a.value).abs() < f64::EPSILON {
                        0.0
                    } else {
                        (value - a.value) / (b.value - a.value)
                    };
                    return gradient_color_from(&[a.color.as_str(), b.color.as_str()], local);
                }
            }
            stops
                .last()
                .map(|stop| stop.color.clone())
                .unwrap_or_else(|| "#000000".to_string())
        }
    }
}

fn default_gradient() -> GradientIr {
    GradientIr::Even(
        CONTINUOUS_GRADIENT
            .iter()
            .map(|stop| (*stop).to_string())
            .collect(),
    )
}

fn binned_colors(scale: Option<&ScaleIr>, min: f64, max: f64) -> Vec<String> {
    if let Some(colors) = scale.and_then(|scale| scale.color_range.clone()) {
        return colors;
    }
    let count = scale
        .and_then(|scale| scale.breaks.as_ref().map(Vec::len))
        .unwrap_or(5)
        .max(1);
    (0..count)
        .map(|index| {
            let t = if count <= 1 {
                0.5
            } else {
                index as f64 / (count - 1) as f64
            };
            gradient_at(&default_gradient(), min + (max - min) * t, min, max, t)
        })
        .collect()
}

fn default_binned_breaks(min: f64, max: f64, count: usize) -> Vec<f64> {
    let count = count.max(1);
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    (0..count)
        .map(|index| min + span * index as f64 / count as f64)
        .collect()
}

fn default_binned_labels(breaks: &[f64]) -> Vec<String> {
    breaks
        .iter()
        .enumerate()
        .map(|(index, lo)| match breaks.get(index + 1) {
            Some(hi) => format!("{}-{}", num(*lo), num(*hi)),
            None => format!("{}+", num(*lo)),
        })
        .collect()
}

pub(crate) fn bin_index_for_value(value: f64, breaks: &[f64]) -> Option<usize> {
    if breaks.is_empty() || value < breaks[0] {
        return None;
    }
    for index in 0..breaks.len() {
        let lo = breaks[index];
        let hi = breaks.get(index + 1).copied();
        if value >= lo && hi.map_or(true, |hi| value < hi) {
            return Some(index);
        }
    }
    None
}

fn is_safe_svg_color(value: &str) -> bool {
    is_hex_color(value) || is_named_color(value)
}

fn is_hex_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_named_color(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "black"
            | "white"
            | "red"
            | "green"
            | "blue"
            | "orange"
            | "purple"
            | "pink"
            | "brown"
            | "gray"
            | "grey"
            | "yellow"
            | "cyan"
            | "magenta"
            | "navy"
            | "teal"
            | "olive"
            | "maroon"
            | "silver"
            | "burlywood"
    )
}

fn gradient_for(scales: &[ScaleIr], aesthetic: &str, column: &str) -> Option<GradientIr> {
    aesthetic_scale(scales, aesthetic, column).and_then(|scale| scale.gradient.clone())
}

fn color_map_for(
    scales: &[ScaleIr],
    aesthetic: &str,
    column: &str,
) -> Option<Vec<(String, String)>> {
    aesthetic_scale(scales, aesthetic, column).and_then(|scale| scale.color_map.clone())
}

fn label_map_for(
    scales: &[ScaleIr],
    aesthetic: &str,
    column: &str,
) -> Option<Vec<(String, String)>> {
    aesthetic_scale(scales, aesthetic, column).and_then(|scale| scale.label_map.clone())
}

fn palette_for(scales: &[ScaleIr], aesthetic: &str, column: &str) -> Option<String> {
    aesthetic_scale(scales, aesthetic, column).and_then(|scale| scale.palette.clone())
}

/// How a numeric aesthetic (`size`/`strokeWidth`) resolves per row: a constant,
/// or a continuous scale from a mapped column's domain into an output range
/// (spec §16.8).
#[derive(Debug, Clone)]
pub enum NumberSpec {
    Constant(f64),
    Scaled {
        col: String,
        domain: (f64, f64),
        range: (f64, f64),
        breaks: Option<Vec<f64>>,
        labels: Option<Vec<String>>,
    },
}

impl NumberSpec {
    /// The resolved value for a row, falling back to `default` for a missing or
    /// non-numeric mapped cell.
    pub fn at(&self, table: &dyn Table, row: usize, default: f64) -> f64 {
        match self {
            NumberSpec::Constant(value) => *value,
            NumberSpec::Scaled {
                col, domain, range, ..
            } => match cell_f64(table, col, row) {
                Some(value) => scale_linear(value, *domain, *range),
                None => default,
            },
        }
    }

    /// A size legend for this aesthetic, if it is a data mapping (spec §19.5).
    /// `kind` selects the swatch shape ([`LegendKind::Width`] for a line of the
    /// mapped thickness, [`LegendKind::Radius`] for a circle of the mapped
    /// radius). Constant settings produce no legend.
    pub fn legend(&self, title: &str, kind: LegendKind) -> Option<Legend> {
        let NumberSpec::Scaled {
            domain,
            range,
            breaks,
            labels,
            ..
        } = self
        else {
            return None;
        };
        let ticks = breaks
            .clone()
            .unwrap_or_else(|| gradient_legend_ticks(domain.0, domain.1));
        if ticks.is_empty() {
            return None;
        }
        let sizes = ticks
            .iter()
            .map(|value| scale_linear(*value, *domain, *range))
            .collect();
        Some(Legend {
            title: title.to_string(),
            kind,
            entries: ticks
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    let label = labels
                        .as_ref()
                        .and_then(|labels| labels.get(index).cloned())
                        .unwrap_or_else(|| num(value));
                    (label, String::new())
                })
                .collect(),
            stroke_entries: Vec::new(),
            sizes,
            shapes: Vec::new(),
            images: Vec::new(),
        })
    }
}

fn scale_linear(value: f64, domain: (f64, f64), range: (f64, f64)) -> f64 {
    let (d0, d1) = domain;
    let t = if (d1 - d0).abs() < f64::EPSILON {
        0.5
    } else {
        ((value - d0) / (d1 - d0)).clamp(0.0, 1.0)
    };
    range.0 + t * (range.1 - range.0)
}

/// Build a [`NumberSpec`] for a numeric aesthetic (`size`/`strokeWidth`). When
/// the aesthetic is mapped to a column, a continuous scale trains from the
/// column's domain (or an explicit `Scale(domain:)`) into `default_range` (or an
/// explicit `Scale(range:)`); otherwise it is the constant setting or
/// `constant_default` (spec §16.8).
pub fn number_spec(
    geo: &GeometryIr,
    aesthetic: PropertyKey,
    table: &dyn Table,
    scales: &[ScaleIr],
    default_range: (f64, f64),
    constant_default: f64,
) -> NumberSpec {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
        let col = mapping.column.name.clone();
        let (data_min, data_max) = numeric_domain(table, &col).unwrap_or((0.0, 1.0));
        let scale = aesthetic_scale(scales, aesthetic.as_str(), &col);
        let domain = match scale.and_then(|s| s.domain) {
            Some([lo, hi]) => (lo.unwrap_or(data_min), hi.unwrap_or(data_max)),
            None => (data_min, data_max),
        };
        let range = match scale.and_then(|s| s.range) {
            Some([lo, hi]) => (lo.unwrap_or(default_range.0), hi.unwrap_or(default_range.1)),
            None => default_range,
        };
        return NumberSpec::Scaled {
            col,
            domain,
            range,
            breaks: scale.and_then(|s| s.breaks.clone()),
            labels: scale.and_then(|s| s.break_labels.clone()),
        };
    }
    NumberSpec::Constant(number_setting(geo, aesthetic, constant_default))
}

pub(crate) fn aesthetic_scale<'a>(
    scales: &'a [ScaleIr],
    aesthetic: &str,
    column: &str,
) -> Option<&'a ScaleIr> {
    scales.iter().rev().find(|scale| match &scale.target {
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: Some(scale_column),
        } => target == aesthetic && scale_column.name == column,
        ScaleTargetIr::Aesthetic {
            aesthetic: target,
            column: None,
        } => target == aesthetic,
        _ => false,
    })
}

/// A constant numeric setting value, or a default.
pub fn number_setting(geo: &GeometryIr, key: PropertyKey, default: f64) -> f64 {
    geo.settings
        .iter()
        .find(|s| s.name == key)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => Some(n),
            _ => None,
        })
        .unwrap_or(default)
}

/// A per-row numeric value: a column mapping resolved at `row` if present,
/// otherwise the constant setting, otherwise `default`. Non-numeric mapped cells
/// fall back to `default` (spec §16.8).
pub fn number_for_row(
    geo: &GeometryIr,
    key: PropertyKey,
    table: &dyn Table,
    row: usize,
    default: f64,
) -> f64 {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == key) {
        return cell_f64(table, &mapping.column.name, row).unwrap_or(default);
    }
    number_setting(geo, key, default)
}
