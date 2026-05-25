//! High-level geometry lowering (spec §15.x): desugar `Histogram`, `FreqPoly`,
//! `Bin2D`, `Density`, and `Bar(stat: "count")` into a synthetic derived table
//! plus a low-level space. This module owns synthetic table names and synthetic
//! output columns; lowered diagnostics keep pointing at the original call.

use algraf_core::Span;
use algraf_data::DataType;

use super::context::Analyzer;
use super::stats::parse_bin_interval;
use crate::ir::*;
use crate::planning::{
    bin2d_output_schema, bin_boundary_dtype, bin_output_schema, count_output_schema,
    density_output_schema,
};
use algraf_core::{codes, Diagnostic};

impl Analyzer<'_> {
    pub(super) fn desugar_histogram(
        &mut self,
        histogram: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let input = self
            .require_numeric_vector(frame, histogram.span, "Histogram", true)?
            .clone();

        let name = self.next_synthetic("histogram");
        let options = self.bin_options_from_geometry(histogram, input.dtype);
        let output_schema = bin_output_schema(input.dtype);
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin,
                input: FrameIr::Vector(input.clone()),
                options,
                span: histogram.span,
            },
            output_schema,
            span: histogram.span,
        };

        let boundary_dtype = bin_boundary_dtype(input.dtype);
        let bin_start = synthetic_column("bin_start", boundary_dtype, histogram.span);
        let bin_end = synthetic_column("bin_end", boundary_dtype, histogram.span);
        let count = synthetic_column("count", DataType::Integer, histogram.span);
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings: vec![
                AestheticMapping {
                    aesthetic: PropertyKey::Xmin,
                    column: bin_start.clone(),
                    span: histogram.span,
                },
                AestheticMapping {
                    aesthetic: PropertyKey::Xmax,
                    column: bin_end,
                    span: histogram.span,
                },
                AestheticMapping {
                    aesthetic: PropertyKey::Ymax,
                    column: count.clone(),
                    span: histogram.span,
                },
            ],
            settings: histogram_rect_settings(histogram),
            span: histogram.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(bin_start), FrameIr::Vector(count)]),
            geometries: vec![rect],
            guides,
            scales,
            theme,
            projection: None,
            span: histogram.span,
        };
        Some((derive, space))
    }

    pub(super) fn desugar_freq_poly(
        &mut self,
        freq_poly: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let input = self
            .require_numeric_vector(frame, freq_poly.span, "FreqPoly", true)?
            .clone();

        let name = self.next_synthetic("freqpoly");
        let options = self.bin_options_from_geometry(freq_poly, input.dtype);
        let output_schema = bin_output_schema(input.dtype);
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin,
                input: FrameIr::Vector(input.clone()),
                options,
                span: freq_poly.span,
            },
            output_schema,
            span: freq_poly.span,
        };

        let boundary_dtype = bin_boundary_dtype(input.dtype);
        let bin_center = synthetic_column("bin_center", boundary_dtype, freq_poly.span);
        let count = synthetic_column("count", DataType::Integer, freq_poly.span);
        let line = GeometryIr {
            kind: GeometryKind::Line,
            mappings: Vec::new(),
            settings: line_settings_from(freq_poly),
            span: freq_poly.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(bin_center), FrameIr::Vector(count)]),
            geometries: vec![line],
            guides,
            scales,
            theme,
            projection: None,
            span: freq_poly.span,
        };
        Some((derive, space))
    }

    pub(super) fn desugar_bin2d(
        &mut self,
        bin2d: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let FrameIr::Cartesian(axes) = frame else {
            self.diag(Diagnostic::error(
                codes::E1302,
                "Bin2D requires a two-dimensional continuous space",
                bin2d.span,
            ));
            return None;
        };
        let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1))
        else {
            self.diag(Diagnostic::error(
                codes::E1302,
                "Bin2D requires two vector dimensions",
                bin2d.span,
            ));
            return None;
        };
        for col in [x, y] {
            if !matches!(
                col.dtype,
                DataType::Integer | DataType::Float | DataType::Unknown
            ) {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("Bin2D input column `{}` is not numeric", col.name),
                    col.span,
                ));
                return None;
            }
        }

        let name = self.next_synthetic("bin2d");
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin2D,
                input: FrameIr::Cartesian(vec![
                    FrameIr::Vector(x.clone()),
                    FrameIr::Vector(y.clone()),
                ]),
                options: StatOptionsIr::Bin2D {
                    bins: bin2d_bins_from_geometry(bin2d),
                },
                span: bin2d.span,
            },
            output_schema: bin2d_output_schema(),
            span: bin2d.span,
        };

        let x_start = synthetic_column("x_start", DataType::Float, bin2d.span);
        let x_end = synthetic_column("x_end", DataType::Float, bin2d.span);
        let y_start = synthetic_column("y_start", DataType::Float, bin2d.span);
        let y_end = synthetic_column("y_end", DataType::Float, bin2d.span);
        let count = synthetic_column("count", DataType::Integer, bin2d.span);
        let mut mappings = vec![
            AestheticMapping {
                aesthetic: PropertyKey::Xmin,
                column: x_start.clone(),
                span: bin2d.span,
            },
            AestheticMapping {
                aesthetic: PropertyKey::Xmax,
                column: x_end.clone(),
                span: bin2d.span,
            },
            AestheticMapping {
                aesthetic: PropertyKey::Ymin,
                column: y_start.clone(),
                span: bin2d.span,
            },
            AestheticMapping {
                aesthetic: PropertyKey::Ymax,
                column: y_end.clone(),
                span: bin2d.span,
            },
        ];
        if !bin2d
            .settings
            .iter()
            .any(|setting| setting.name == PropertyKey::Fill)
        {
            mappings.push(AestheticMapping {
                aesthetic: PropertyKey::Fill,
                column: count,
                span: bin2d.span,
            });
        }
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings,
            settings: bin2d_rect_settings(bin2d),
            span: bin2d.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![
                FrameIr::Union(vec![FrameIr::Vector(x_start), FrameIr::Vector(x_end)]),
                FrameIr::Union(vec![FrameIr::Vector(y_start), FrameIr::Vector(y_end)]),
            ]),
            geometries: vec![rect],
            guides,
            scales,
            theme,
            projection: None,
            span: bin2d.span,
        };
        Some((derive, space))
    }

    /// Desugar `Density()` over a 1D numeric vector space into a kernel-density
    /// derived table and a 2D `Area` space (spec §15.11). The KDE produces
    /// `density_x` and `density` columns; the area is drawn from the curve down
    /// to a zero baseline, mirroring how `Histogram` desugars to `Rect`.
    pub(super) fn desugar_density(
        &mut self,
        density: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let input = self
            .require_numeric_vector(frame, density.span, "Density", false)?
            .clone();

        let name = self.next_synthetic("density");
        let options = self.density_options(density);
        let output_schema = density_output_schema();
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Density,
                input: FrameIr::Vector(input.clone()),
                options,
                span: density.span,
            },
            output_schema,
            span: density.span,
        };

        let density_x = synthetic_column("density_x", DataType::Float, density.span);
        let density_y = synthetic_column("density", DataType::Float, density.span);
        let area = GeometryIr {
            kind: GeometryKind::Area,
            mappings: Vec::new(),
            settings: density_area_settings(density),
            span: density.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(density_x), FrameIr::Vector(density_y)]),
            geometries: vec![area],
            guides,
            scales,
            theme,
            projection: None,
            span: density.span,
        };
        Some((derive, space))
    }

    /// Require a single-column numeric vector space for a 1D lowering target,
    /// returning the input column. Emits `E1302` for a non-vector space and
    /// `E1404` for a non-numeric column. `allow_temporal` admits temporal
    /// columns (binning) but not density estimation (spec §15.x).
    fn require_numeric_vector<'f>(
        &mut self,
        frame: &'f FrameIr,
        span: Span,
        label: &str,
        allow_temporal: bool,
    ) -> Option<&'f ColumnRef> {
        let FrameIr::Vector(input) = frame else {
            self.diag(Diagnostic::error(
                codes::E1302,
                format!("{label} requires a single numeric vector space"),
                span,
            ));
            return None;
        };
        let numeric = matches!(
            input.dtype,
            DataType::Integer | DataType::Float | DataType::Unknown
        );
        if numeric || (allow_temporal && input.dtype == DataType::Temporal) {
            Some(input)
        } else {
            let kinds = if allow_temporal {
                "numeric or temporal"
            } else {
                "numeric"
            };
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("{label} input column `{}` is not {kinds}", input.name),
                input.span,
            ));
            None
        }
    }

    /// Build typed `Density` options from a `Density(...)` geometry's settings,
    /// re-validating ranges against the original call span (spec §15.11).
    fn density_options(&mut self, density: &GeometryIr) -> StatOptionsIr {
        let mut bandwidth = None;
        let mut grid_points = None;
        for setting in &density.settings {
            match (setting.name, &setting.value) {
                (PropertyKey::Bandwidth, SettingValue::Number(n)) => bandwidth = Some(*n),
                (PropertyKey::N, SettingValue::Number(n)) => grid_points = Some(*n),
                _ => {}
            }
        }
        if bandwidth.is_some_and(|value| value <= 0.0) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`bandwidth` must be greater than 0",
                density.span,
            ));
        }
        if grid_points.is_some_and(|value| value < 2.0) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`n` must be at least 2",
                density.span,
            ));
        }
        StatOptionsIr::Density {
            bandwidth,
            grid_points,
        }
    }

    /// Desugar `Bar(stat: "count")` over a 1D categorical space into a Count
    /// derived table and a 2D `Bar` space (spec §15.5).
    pub(super) fn desugar_count_bar(
        &mut self,
        bar: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        // Find the categorical group column(s). For 0.1, support 1D categorical
        // space (`Space(category)`) and nested 1D (`Space(outer / inner)`).
        let group_cols: Vec<&ColumnRef> = match frame {
            FrameIr::Vector(column) => vec![column],
            FrameIr::Nested { outer, inner } => match (outer.as_ref(), inner.as_ref()) {
                (FrameIr::Vector(o), FrameIr::Vector(i)) => vec![o, i],
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1302,
                        "Bar(stat: \"count\") requires a 1D categorical space",
                        bar.span,
                    ));
                    return None;
                }
            },
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1302,
                    "Bar(stat: \"count\") requires a 1D categorical space",
                    bar.span,
                ));
                return None;
            }
        };

        // Only desugar when reading the primary table; counts over derived
        // tables are not meaningful in 0.1.
        if !matches!(data_ref, SpaceDataRef::Primary) {
            self.diag(Diagnostic::error(
                codes::E1302,
                "Bar(stat: \"count\") must read from the primary table",
                bar.span,
            ));
            return None;
        }

        let name = self.next_synthetic("count");

        let output_schema = count_output_schema(
            &group_cols
                .iter()
                .map(|column| (*column).clone())
                .collect::<Vec<_>>(),
        );

        // The stat input frame is just the categorical key(s).
        let stat_input = if group_cols.len() == 1 {
            FrameIr::Vector((*group_cols[0]).clone())
        } else {
            FrameIr::Nested {
                outer: Box::new(FrameIr::Vector((*group_cols[0]).clone())),
                inner: Box::new(FrameIr::Vector((*group_cols[1]).clone())),
            }
        };

        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Count,
                input: stat_input,
                options: StatOptionsIr::Count,
                span: bar.span,
            },
            output_schema,
            span: bar.span,
        };

        // The derived-table-backed space mirrors the input keys on x and uses
        // `count` for y.
        let count_col = synthetic_column("count", DataType::Integer, bar.span);
        let x_frame = if group_cols.len() == 1 {
            FrameIr::Vector(synthetic_column(
                &group_cols[0].name,
                group_cols[0].dtype,
                bar.span,
            ))
        } else {
            FrameIr::Nested {
                outer: Box::new(FrameIr::Vector(synthetic_column(
                    &group_cols[0].name,
                    group_cols[0].dtype,
                    bar.span,
                ))),
                inner: Box::new(FrameIr::Vector(synthetic_column(
                    &group_cols[1].name,
                    group_cols[1].dtype,
                    bar.span,
                ))),
            }
        };

        // Preserve mappings/settings from the original Bar (e.g. fill, alpha).
        // The y resolution comes from the derived `count` column via the
        // synthetic Cartesian frame; no explicit `y` mapping is needed.
        let mappings = bar.mappings.clone();
        let settings = bar
            .settings
            .iter()
            .filter(|s| s.name != PropertyKey::Stat)
            .cloned()
            .collect();

        let bar_ir = GeometryIr {
            kind: GeometryKind::Bar,
            mappings,
            settings,
            span: bar.span,
        };

        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![x_frame, FrameIr::Vector(count_col)]),
            geometries: vec![bar_ir],
            guides,
            scales,
            theme,
            projection: None,
            span: bar.span,
        };
        Some((derive, space))
    }

    /// Build typed `Bin` options from a `Histogram`/`FreqPoly` geometry's
    /// settings, re-validating ranges and the `bins`/`binWidth` conflict against
    /// the original call span (spec §15.x). Property types were already checked
    /// by the geometry registry, so only ranges are re-checked here.
    fn bin_options_from_geometry(
        &mut self,
        geometry: &GeometryIr,
        input_dtype: DataType,
    ) -> StatOptionsIr {
        let mut bins = None;
        let mut bin_width = None;
        let mut boundary = None;
        let mut closed = BinClosedIr::Left;
        let mut interval = None;
        for setting in &geometry.settings {
            match (setting.name, &setting.value) {
                (PropertyKey::Bins, SettingValue::Number(n)) => bins = Some(*n),
                (PropertyKey::BinWidth, SettingValue::Number(n)) => bin_width = Some(*n),
                (PropertyKey::Boundary, SettingValue::Number(n)) => boundary = Some(*n),
                (PropertyKey::Closed, SettingValue::String(s)) if s == "right" => {
                    closed = BinClosedIr::Right
                }
                (PropertyKey::Closed, SettingValue::String(s)) if s == "left" => {
                    closed = BinClosedIr::Left
                }
                (PropertyKey::Interval, SettingValue::String(s)) => {
                    interval = parse_bin_interval(s);
                }
                _ => {}
            }
        }
        if bins.is_some_and(|value| value < 1.0) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`bins` must be at least 1",
                geometry.span,
            ));
        }
        if bin_width.is_some_and(|value| value <= 0.0) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`binWidth` must be greater than 0",
                geometry.span,
            ));
        }
        if interval.is_some() && !matches!(input_dtype, DataType::Temporal | DataType::Unknown) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`interval` applies only to temporal histogram inputs",
                geometry.span,
            ));
        }
        self.check_bin_conflict(
            bins.is_some(),
            bin_width.is_some(),
            boundary.is_some(),
            interval.is_some(),
            geometry.span,
        );
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
            interval,
        }
    }
}

fn bin2d_bins_from_geometry(bin2d: &GeometryIr) -> Option<f64> {
    bin2d
        .settings
        .iter()
        .find_map(|setting| match &setting.value {
            SettingValue::Number(n) if setting.name == PropertyKey::Bins => Some(*n),
            _ => None,
        })
}

fn synthetic_column(name: &str, dtype: DataType, span: Span) -> ColumnRef {
    ColumnRef {
        name: name.into(),
        dtype,
        span,
    }
}

/// Visual settings copied verbatim from a high-level geometry onto the
/// low-level mark it lowers into (fill area / rect / line).
const FILL_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Alpha,
];
const STROKE_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Alpha,
];

/// Copy the `allow`-listed settings from `geometry` in source order, preserving
/// their values and spans. Used to pass a high-level geometry's visual settings
/// through to the low-level mark it desugars into.
fn passthrough_settings(geometry: &GeometryIr, allow: &[PropertyKey]) -> Vec<GeometrySetting> {
    geometry
        .settings
        .iter()
        .filter(|setting| allow.contains(&setting.name))
        .cloned()
        .collect()
}

fn fixed_setting(name: PropertyKey, value: f64, span: Span) -> GeometrySetting {
    GeometrySetting {
        name,
        value: SettingValue::Number(value),
        span,
    }
}

fn histogram_rect_settings(histogram: &GeometryIr) -> Vec<GeometrySetting> {
    let mut settings = vec![fixed_setting(PropertyKey::Ymin, 0.0, histogram.span)];
    settings.extend(passthrough_settings(histogram, FILL_SETTINGS));
    settings
}

fn line_settings_from(geometry: &GeometryIr) -> Vec<GeometrySetting> {
    passthrough_settings(geometry, STROKE_SETTINGS)
}

fn bin2d_rect_settings(bin2d: &GeometryIr) -> Vec<GeometrySetting> {
    passthrough_settings(bin2d, FILL_SETTINGS)
}

/// Pass the visual settings of a `Density` geometry through to the `Area` it
/// desugars into. The KDE curve is filled to a zero baseline.
fn density_area_settings(density: &GeometryIr) -> Vec<GeometrySetting> {
    let mut settings = vec![fixed_setting(PropertyKey::Baseline, 0.0, density.span)];
    settings.extend(passthrough_settings(density, FILL_SETTINGS));
    settings
}
