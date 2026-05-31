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
        annotations: Vec<GeometryIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        // Overlay: `Space((a + b)) { Histogram(...) }` blends multiple numeric
        // columns onto one shared bin axis and draws full-width, alpha-blended
        // bars colored by a synthetic `series` column (spec §14.7).
        if let FrameIr::Union(members) = frame {
            if members.len() < 2 {
                self.diag(Diagnostic::error(
                    codes::E1302,
                    "blended Histogram requires at least two numeric columns",
                    histogram.span,
                ));
                return None;
            }
            let mut columns = Vec::new();
            for member in members {
                columns.push(
                    self.require_numeric_vector(
                        member,
                        histogram.span,
                        "blended Histogram",
                        false,
                    )?
                    .clone(),
                );
            }
            return Some(self.blended_histogram(
                histogram,
                columns,
                theme,
                guides,
                scales,
                annotations,
            ));
        }

        // Dodge: `Space(value / group)` nests the group inside the binned value
        // axis, so each bin is split into side-by-side per-group sub-bars
        // (spec §14.5). The nest is the trigger — no `layout` keyword.
        if let FrameIr::Nested { outer, inner } = frame {
            let value = self
                .require_numeric_vector(outer, histogram.span, "Histogram", false)?
                .clone();
            let FrameIr::Vector(group) = inner.as_ref() else {
                self.diag(Diagnostic::error(
                    codes::E1302,
                    "grouped Histogram requires `value / group` with a single group column",
                    histogram.span,
                ));
                return None;
            };
            return Some(self.grouped_histogram(
                histogram,
                value,
                group.clone(),
                true,
                theme,
                guides,
                scales,
                annotations,
            ));
        }

        let input = self
            .require_numeric_vector(frame, histogram.span, "Histogram", true)?
            .clone();

        // Stacked grouping: an explicit `group` mapping, else a `fill` column
        // mapping (spec §15.6). A literal `fill: "color"` is a setting, not a
        // mapping, so it does not trigger grouping.
        let group = histogram
            .mappings
            .iter()
            .find(|m| m.aesthetic == PropertyKey::Group)
            .or_else(|| {
                histogram
                    .mappings
                    .iter()
                    .find(|m| m.aesthetic == PropertyKey::Fill)
            })
            .map(|m| m.column.clone());

        if let Some(group) = group {
            if input.dtype == DataType::Temporal {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    "grouped Histogram requires a numeric input column",
                    histogram.span,
                ));
                return None;
            }
            return Some(self.grouped_histogram(
                histogram,
                input,
                group,
                false,
                theme,
                guides,
                scales,
                annotations,
            ));
        }

        let name = self.next_synthetic("histogram");
        let options = self.bin_options_from_geometry(histogram, input.dtype);
        let output_schema = bin_output_schema(input.dtype);
        let derive = stat_derive(
            name.clone(),
            StatKind::Bin,
            FrameIr::Vector(input.clone()),
            options,
            output_schema,
            histogram.span,
        );

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
            interaction: InteractionIr::default(),
            span: histogram.span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![FrameIr::Vector(bin_start), FrameIr::Vector(count)]),
            with_annotations(rect, annotations),
            theme,
            guides,
            scales,
            histogram.span,
        );
        Some((derive, space))
    }

    pub(super) fn desugar_interval_sugar(
        &mut self,
        geometry: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(Vec<DeriveIr>, Vec<SpaceIr>)> {
        match geometry.kind {
            GeometryKind::ErrorBar => {
                self.desugar_error_bar(geometry, frame, data_ref, theme, guides, scales)
            }
            GeometryKind::LineRange => {
                self.desugar_line_range(geometry, frame, data_ref, theme, guides, scales)
            }
            GeometryKind::PointRange => {
                self.desugar_point_range(geometry, frame, data_ref, theme, guides, scales)
            }
            GeometryKind::CrossBar => {
                self.desugar_cross_bar(geometry, frame, data_ref, theme, guides, scales)
            }
            _ => None,
        }
    }

    fn desugar_error_bar(
        &mut self,
        geometry: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(Vec<DeriveIr>, Vec<SpaceIr>)> {
        let parts = self.interval_parts(geometry, frame)?;
        let cap_width = number_setting_from_geometry(geometry, PropertyKey::CapWidth);
        let (derive, space) = self.interval_segment_space(
            geometry,
            data_ref,
            parts.position,
            parts.lower,
            parts.upper,
            parts.orientation,
            cap_width,
            theme,
            guides,
            scales,
        );
        Some((vec![derive], vec![space]))
    }

    fn desugar_line_range(
        &mut self,
        geometry: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(Vec<DeriveIr>, Vec<SpaceIr>)> {
        let parts = self.interval_parts(geometry, frame)?;
        let (derive, space) = self.interval_segment_space(
            geometry,
            data_ref,
            parts.position,
            parts.lower,
            parts.upper,
            parts.orientation,
            None,
            theme,
            guides,
            scales,
        );
        Some((vec![derive], vec![space]))
    }

    fn desugar_point_range(
        &mut self,
        geometry: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(Vec<DeriveIr>, Vec<SpaceIr>)> {
        let parts = self.interval_parts(geometry, frame)?;
        let (derive, segment_space) = self.interval_segment_space(
            geometry,
            data_ref,
            parts.position,
            parts.lower,
            parts.upper,
            parts.orientation,
            None,
            theme.clone(),
            guides.clone(),
            scales.clone(),
        );
        let point = GeometryIr {
            kind: GeometryKind::Point,
            mappings: passthrough_mappings(geometry, POINT_RANGE_POINT_MAPPINGS),
            settings: passthrough_settings(geometry, POINT_RANGE_POINT_SETTINGS),
            interaction: InteractionIr::default(),
            span: geometry.span,
        };
        let point_space = SpaceIr {
            data: data_ref.clone(),
            frame: frame.clone(),
            layers: vec![SpaceLayerIr::Geometry(point.clone())],
            geometries: vec![point],
            guides,
            scales,
            theme,
            projection: None,
            coords: CoordsIr::Cartesian,
            view: CoordinateViewIr::default(),
            span: geometry.span,
        };
        Some((vec![derive], vec![segment_space, point_space]))
    }

    fn desugar_cross_bar(
        &mut self,
        geometry: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(Vec<DeriveIr>, Vec<SpaceIr>)> {
        let parts = self.interval_parts(geometry, frame)?;
        let middle = self.interval_middle_column(frame, parts.orientation, geometry.span)?;
        let width = number_setting_from_geometry(geometry, PropertyKey::Width);
        let rect_name = self.next_synthetic("interval_rects");
        let middle_name = self.next_synthetic("interval_middles");
        let rect_input = FrameIr::Cartesian(vec![
            FrameIr::Vector(parts.position.clone()),
            FrameIr::Vector(parts.lower.clone()),
            FrameIr::Vector(parts.upper.clone()),
        ]);
        let middle_input = FrameIr::Cartesian(vec![
            FrameIr::Vector(parts.position),
            FrameIr::Vector(middle),
        ]);
        let rect_output =
            crate::planning::interval_rects_output_schema(&rect_input, parts.orientation);
        let middle_output =
            crate::planning::interval_middles_output_schema(&middle_input, parts.orientation);
        let rect_derive = stat_derive_for_data(
            rect_name.clone(),
            data_ref.clone(),
            StatKind::IntervalRects,
            rect_input,
            StatOptionsIr::IntervalRects {
                orientation: parts.orientation,
                width,
            },
            rect_output.clone(),
            geometry.span,
        );
        let middle_derive = stat_derive_for_data(
            middle_name.clone(),
            data_ref.clone(),
            StatKind::IntervalMiddles,
            middle_input,
            StatOptionsIr::IntervalMiddles {
                orientation: parts.orientation,
                width,
            },
            middle_output.clone(),
            geometry.span,
        );

        let xmin_dtype = schema_dtype(&rect_output, "xmin");
        let xmax_dtype = schema_dtype(&rect_output, "xmax");
        let ymin_dtype = schema_dtype(&rect_output, "ymin");
        let ymax_dtype = schema_dtype(&rect_output, "ymax");
        let middle_x_dtype = schema_dtype(&middle_output, "x");
        let middle_y_dtype = schema_dtype(&middle_output, "y");
        let xmin = synthetic_column("xmin", xmin_dtype, geometry.span);
        let xmax = synthetic_column("xmax", xmax_dtype, geometry.span);
        let ymin = synthetic_column("ymin", ymin_dtype, geometry.span);
        let ymax = synthetic_column("ymax", ymax_dtype, geometry.span);
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings: vec![
                mapping(PropertyKey::Xmin, "xmin", xmin_dtype, geometry.span),
                mapping(PropertyKey::Xmax, "xmax", xmax_dtype, geometry.span),
                mapping(PropertyKey::Ymin, "ymin", ymin_dtype, geometry.span),
                mapping(PropertyKey::Ymax, "ymax", ymax_dtype, geometry.span),
            ]
            .into_iter()
            .chain(passthrough_mappings(geometry, CROSS_BAR_RECT_MAPPINGS))
            .collect(),
            settings: passthrough_settings(geometry, CROSS_BAR_RECT_SETTINGS),
            interaction: InteractionIr::default(),
            span: geometry.span,
        };
        let rect_space = derived_space(
            rect_name,
            FrameIr::Cartesian(vec![
                FrameIr::Union(vec![FrameIr::Vector(xmin), FrameIr::Vector(xmax)]),
                FrameIr::Union(vec![FrameIr::Vector(ymin), FrameIr::Vector(ymax)]),
            ]),
            vec![rect],
            theme.clone(),
            guides.clone(),
            scales.clone(),
            geometry.span,
        );

        let segment = GeometryIr {
            kind: GeometryKind::Segment,
            mappings: segment_endpoint_mappings(geometry.span, middle_x_dtype, middle_y_dtype)
                .into_iter()
                .chain(passthrough_mappings(geometry, INTERVAL_SEGMENT_MAPPINGS))
                .collect(),
            settings: passthrough_settings(geometry, INTERVAL_SEGMENT_SETTINGS),
            interaction: InteractionIr::default(),
            span: geometry.span,
        };
        let middle_space = derived_space(
            middle_name,
            FrameIr::Cartesian(vec![
                FrameIr::Vector(synthetic_column("x", middle_x_dtype, geometry.span)),
                FrameIr::Vector(synthetic_column("y", middle_y_dtype, geometry.span)),
            ]),
            vec![segment],
            theme,
            guides,
            scales,
            geometry.span,
        );
        Some((
            vec![rect_derive, middle_derive],
            vec![rect_space, middle_space],
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn interval_segment_space(
        &mut self,
        geometry: &GeometryIr,
        data_ref: &SpaceDataRef,
        position: ColumnRef,
        lower: ColumnRef,
        upper: ColumnRef,
        orientation: IntervalOrientationIr,
        cap_width: Option<f64>,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> (DeriveIr, SpaceIr) {
        let name = self.next_synthetic("interval_segments");
        let input = FrameIr::Cartesian(vec![
            FrameIr::Vector(position),
            FrameIr::Vector(lower),
            FrameIr::Vector(upper),
        ]);
        let output_schema = crate::planning::interval_segments_output_schema(&input, orientation);
        let x_dtype = schema_dtype(&output_schema, "x");
        let y_dtype = schema_dtype(&output_schema, "y");
        let derive = stat_derive_for_data(
            name.clone(),
            data_ref.clone(),
            StatKind::IntervalSegments,
            input,
            StatOptionsIr::IntervalSegments {
                orientation,
                cap_width,
            },
            output_schema,
            geometry.span,
        );
        let segment = GeometryIr {
            kind: GeometryKind::Segment,
            mappings: segment_endpoint_mappings(geometry.span, x_dtype, y_dtype)
                .into_iter()
                .chain(passthrough_mappings(geometry, INTERVAL_SEGMENT_MAPPINGS))
                .collect(),
            settings: passthrough_settings(geometry, INTERVAL_SEGMENT_SETTINGS),
            interaction: InteractionIr::default(),
            span: geometry.span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![
                FrameIr::Vector(synthetic_column("x", x_dtype, geometry.span)),
                FrameIr::Vector(synthetic_column("y", y_dtype, geometry.span)),
            ]),
            vec![segment],
            theme,
            guides,
            scales,
            geometry.span,
        );
        (derive, space)
    }

    fn interval_parts(&mut self, geometry: &GeometryIr, frame: &FrameIr) -> Option<IntervalParts> {
        let orientation = self.interval_orientation_from_geometry(geometry)?;
        let position = self.interval_position_column(frame, orientation, geometry.span)?;
        let (lower_key, upper_key) = match orientation {
            IntervalOrientationIr::Vertical => (PropertyKey::Ymin, PropertyKey::Ymax),
            IntervalOrientationIr::Horizontal => (PropertyKey::Xmin, PropertyKey::Xmax),
        };
        let Some(lower) = mapping_column(geometry, lower_key) else {
            self.diag(Diagnostic::error(
                codes::E1205,
                format!(
                    "`{}` requires property `{}`",
                    geometry.kind.display_name(),
                    lower_key.as_str()
                ),
                geometry.span,
            ));
            return None;
        };
        let Some(upper) = mapping_column(geometry, upper_key) else {
            self.diag(Diagnostic::error(
                codes::E1205,
                format!(
                    "`{}` requires property `{}`",
                    geometry.kind.display_name(),
                    upper_key.as_str()
                ),
                geometry.span,
            ));
            return None;
        };
        Some(IntervalParts {
            orientation,
            position,
            lower,
            upper,
        })
    }

    fn interval_orientation_from_geometry(
        &mut self,
        geometry: &GeometryIr,
    ) -> Option<IntervalOrientationIr> {
        if let Some(setting) = geometry
            .settings
            .iter()
            .find(|setting| setting.name == PropertyKey::Orientation)
        {
            if let SettingValue::String(value) = &setting.value {
                return match value.as_str() {
                    "vertical" => Some(IntervalOrientationIr::Vertical),
                    "horizontal" => Some(IntervalOrientationIr::Horizontal),
                    _ => None,
                };
            }
        }
        let has_y = mapping_column(geometry, PropertyKey::Ymin).is_some()
            || mapping_column(geometry, PropertyKey::Ymax).is_some();
        let has_x = mapping_column(geometry, PropertyKey::Xmin).is_some()
            || mapping_column(geometry, PropertyKey::Xmax).is_some();
        match (has_y, has_x) {
            (true, false) => Some(IntervalOrientationIr::Vertical),
            (false, true) => Some(IntervalOrientationIr::Horizontal),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1205,
                    format!(
                        "`{}` needs either `ymin`/`ymax` or `xmin`/`xmax` bounds",
                        geometry.kind.display_name()
                    ),
                    geometry.span,
                ));
                None
            }
        }
    }

    fn interval_position_column(
        &mut self,
        frame: &FrameIr,
        orientation: IntervalOrientationIr,
        span: Span,
    ) -> Option<ColumnRef> {
        let (x, y) = self.interval_frame_axes(frame, span)?;
        Some(match orientation {
            IntervalOrientationIr::Vertical => x,
            IntervalOrientationIr::Horizontal => y,
        })
    }

    fn interval_middle_column(
        &mut self,
        frame: &FrameIr,
        orientation: IntervalOrientationIr,
        span: Span,
    ) -> Option<ColumnRef> {
        let (x, y) = self.interval_frame_axes(frame, span)?;
        Some(match orientation {
            IntervalOrientationIr::Vertical => y,
            IntervalOrientationIr::Horizontal => x,
        })
    }

    fn interval_frame_axes(
        &mut self,
        frame: &FrameIr,
        span: Span,
    ) -> Option<(ColumnRef, ColumnRef)> {
        let FrameIr::Cartesian(axes) = frame else {
            self.diag(Diagnostic::error(
                codes::E1302,
                "interval sugar requires a two-dimensional Cartesian space",
                span,
            ));
            return None;
        };
        let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1))
        else {
            self.diag(Diagnostic::error(
                codes::E1302,
                "interval sugar requires two vector dimensions",
                span,
            ));
            return None;
        };
        Some((x.clone(), y.clone()))
    }

    /// Desugar a blended `Histogram` into a `Bin` over a union of numeric
    /// columns plus overlaid `Rect`s colored by synthetic `series`.
    #[allow(clippy::too_many_arguments)]
    fn blended_histogram(
        &mut self,
        histogram: &GeometryIr,
        values: Vec<ColumnRef>,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
        annotations: Vec<GeometryIr>,
    ) -> (DeriveIr, SpaceIr) {
        let span = histogram.span;
        let name = self.next_synthetic("histogram");
        let options = self.bin_options_from_geometry(histogram, DataType::Float);
        let derive = stat_derive(
            name.clone(),
            StatKind::Bin,
            FrameIr::Union(values.into_iter().map(FrameIr::Vector).collect()),
            options,
            crate::planning::blended_bin_output_schema(),
            span,
        );

        let bin_start = synthetic_column("bin_start", DataType::Float, span);
        let count = synthetic_column("count", DataType::Integer, span);
        let series = synthetic_column("series", DataType::String, span);
        let mapping = |aesthetic: PropertyKey, name: &str, dtype: DataType| AestheticMapping {
            aesthetic,
            column: synthetic_column(name, dtype, span),
            span,
        };
        let mut settings = vec![fixed_setting(PropertyKey::Ymin, 0.0, span)];
        settings.extend(passthrough_settings(histogram, GROUPED_RECT_SETTINGS));
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings: vec![
                mapping(PropertyKey::Xmin, "bin_start", DataType::Float),
                mapping(PropertyKey::Xmax, "bin_end", DataType::Float),
                mapping(PropertyKey::Ymax, "count", DataType::Integer),
                AestheticMapping {
                    aesthetic: PropertyKey::Fill,
                    column: series,
                    span,
                },
            ],
            settings,
            interaction: InteractionIr::default(),
            span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![FrameIr::Vector(bin_start), FrameIr::Vector(count)]),
            with_annotations(rect, annotations),
            theme,
            guides,
            scales,
            span,
        );
        (derive, space)
    }

    /// Desugar a grouped `Histogram` into a grouped `Bin` plus `Rect`s colored by
    /// the group column (spec §15.6). The `Bin` stat receives a two-column
    /// `(value, group)` input and emits both pre-stacked y-bounds and per-group
    /// dodge sub-slots. When `dodge` is set (the `value / group` nest form), each
    /// bin is split into side-by-side sub-bars from a zero baseline; otherwise
    /// the groups stack. The group column drives a categorical `fill` scale.
    // Mirrors the other `desugar_*` signatures (geometry + space context) with
    // the grouped extras `value`/`group`/`dodge`; the arity is inherent.
    #[allow(clippy::too_many_arguments)]
    fn grouped_histogram(
        &mut self,
        histogram: &GeometryIr,
        value: ColumnRef,
        group: ColumnRef,
        dodge: bool,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
        annotations: Vec<GeometryIr>,
    ) -> (DeriveIr, SpaceIr) {
        let span = histogram.span;
        let name = self.next_synthetic("histogram");
        let options = self.bin_options_from_geometry(histogram, value.dtype);
        let output_schema = crate::planning::grouped_bin_output_schema(&group.name);
        let derive = stat_derive(
            name.clone(),
            StatKind::Bin,
            FrameIr::Cartesian(vec![FrameIr::Vector(value), FrameIr::Vector(group.clone())]),
            options,
            output_schema,
            span,
        );

        let bin_start = synthetic_column("bin_start", DataType::Float, span);
        let count = synthetic_column("count", DataType::Integer, span);
        let group_col = ColumnRef {
            name: group.name.clone(),
            dtype: DataType::String,
            span,
        };
        let mapping = |aesthetic: PropertyKey, name: &str, dtype: DataType| AestheticMapping {
            aesthetic,
            column: synthetic_column(name, dtype, span),
            span,
        };

        // Dodge maps x to the sub-slot and y from a zero baseline to the group
        // count; stack maps x to the full bin and y to the cumulative bounds.
        let (mappings, mut settings) = if dodge {
            (
                vec![
                    mapping(PropertyKey::Xmin, "dodge_start", DataType::Float),
                    mapping(PropertyKey::Xmax, "dodge_end", DataType::Float),
                    mapping(PropertyKey::Ymax, "count", DataType::Integer),
                    AestheticMapping {
                        aesthetic: PropertyKey::Fill,
                        column: group_col,
                        span,
                    },
                ],
                vec![fixed_setting(PropertyKey::Ymin, 0.0, span)],
            )
        } else {
            (
                vec![
                    mapping(PropertyKey::Xmin, "bin_start", DataType::Float),
                    mapping(PropertyKey::Xmax, "bin_end", DataType::Float),
                    mapping(PropertyKey::Ymin, "stack_lower", DataType::Float),
                    mapping(PropertyKey::Ymax, "stack_upper", DataType::Float),
                    AestheticMapping {
                        aesthetic: PropertyKey::Fill,
                        column: group_col,
                        span,
                    },
                ],
                Vec::new(),
            )
        };
        // Pass through stroke/strokeWidth/alpha; `fill` is a mapping here.
        settings.extend(passthrough_settings(histogram, GROUPED_RECT_SETTINGS));
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings,
            settings,
            interaction: InteractionIr::default(),
            span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![FrameIr::Vector(bin_start), FrameIr::Vector(count)]),
            with_annotations(rect, annotations),
            theme,
            guides,
            scales,
            span,
        );
        (derive, space)
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
        let derive = stat_derive(
            name.clone(),
            StatKind::Bin,
            FrameIr::Vector(input.clone()),
            options,
            output_schema,
            freq_poly.span,
        );

        let boundary_dtype = bin_boundary_dtype(input.dtype);
        let bin_center = synthetic_column("bin_center", boundary_dtype, freq_poly.span);
        let count = synthetic_column("count", DataType::Integer, freq_poly.span);
        let line = GeometryIr {
            kind: GeometryKind::Line,
            mappings: Vec::new(),
            settings: line_settings_from(freq_poly),
            interaction: InteractionIr::default(),
            span: freq_poly.span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![FrameIr::Vector(bin_center), FrameIr::Vector(count)]),
            vec![line],
            theme,
            guides,
            scales,
            freq_poly.span,
        );
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
        let derive = stat_derive(
            name.clone(),
            StatKind::Bin2D,
            FrameIr::Cartesian(vec![FrameIr::Vector(x.clone()), FrameIr::Vector(y.clone())]),
            StatOptionsIr::Bin2D {
                bins: bin2d_bins_from_geometry(bin2d),
            },
            bin2d_output_schema(),
            bin2d.span,
        );

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
            interaction: InteractionIr::default(),
            span: bin2d.span,
        };
        let space = derived_space(
            name,
            FrameIr::Cartesian(vec![
                FrameIr::Union(vec![FrameIr::Vector(x_start), FrameIr::Vector(x_end)]),
                FrameIr::Union(vec![FrameIr::Vector(y_start), FrameIr::Vector(y_end)]),
            ]),
            vec![rect],
            theme,
            guides,
            scales,
            bin2d.span,
        );
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
        // Overlay: `Space((a + b)) { Density(...) }` blends multiple numeric
        // columns onto a shared density axis colored by synthetic `series` (spec §14.9).
        if let FrameIr::Union(members) = frame {
            if members.len() < 2 {
                self.diag(Diagnostic::error(
                    codes::E1302,
                    "blended Density requires at least two numeric columns",
                    density.span,
                ));
                return None;
            }
            let mut columns = Vec::new();
            for member in members {
                columns.push(
                    self.require_numeric_vector(member, density.span, "blended Density", false)?
                        .clone(),
                );
            }
            return Some(self.blended_density(density, columns, theme, guides, scales));
        }

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
            interaction: InteractionIr::default(),
            span: density.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(density_x), FrameIr::Vector(density_y)]),
            layers: vec![SpaceLayerIr::Geometry(area.clone())],
            geometries: vec![area],
            guides,
            scales,
            theme,
            projection: None,
            // Inherits the parent space's coordinate system (set in `space()`).
            coords: CoordsIr::Cartesian,
            view: CoordinateViewIr::default(),
            span: density.span,
        };
        Some((derive, space))
    }

    /// Desugar a blended `Density` into a `Density` over a union of numeric
    /// columns plus an overlaid `Area` colored by synthetic `series`.
    fn blended_density(
        &mut self,
        density: &GeometryIr,
        values: Vec<ColumnRef>,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> (DeriveIr, SpaceIr) {
        let span = density.span;
        let name = self.next_synthetic("density");
        let options = self.density_options(density);
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Density,
                input: FrameIr::Union(values.into_iter().map(FrameIr::Vector).collect()),
                options,
                span,
            },
            output_schema: crate::planning::blended_density_output_schema(),
            span,
        };

        let density_x = synthetic_column("density_x", DataType::Float, span);
        let density_y = synthetic_column("density", DataType::Float, span);
        let series = synthetic_column("series", DataType::String, span);
        let mappings = vec![AestheticMapping {
            aesthetic: PropertyKey::Fill,
            column: series,
            span,
        }];
        let area = GeometryIr {
            kind: GeometryKind::Area,
            mappings,
            settings: density_area_settings(density),
            interaction: InteractionIr::default(),
            span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(density_x), FrameIr::Vector(density_y)]),
            layers: vec![SpaceLayerIr::Geometry(area.clone())],
            geometries: vec![area],
            guides,
            scales,
            theme,
            projection: None,
            coords: CoordsIr::Cartesian,
            view: CoordinateViewIr::default(),
            span,
        };
        (derive, space)
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
            interaction: bar.interaction.clone(),
            span: bar.span,
        };

        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![x_frame, FrameIr::Vector(count_col)]),
            layers: vec![SpaceLayerIr::Geometry(bar_ir.clone())],
            geometries: vec![bar_ir],
            guides,
            scales,
            theme,
            projection: None,
            // Inherits the parent space's coordinate system (set in `space()`).
            coords: CoordsIr::Cartesian,
            view: CoordinateViewIr::default(),
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

fn stat_derive(
    name: String,
    kind: StatKind,
    input: FrameIr,
    options: StatOptionsIr,
    output_schema: Vec<ColumnDefIr>,
    span: Span,
) -> DeriveIr {
    stat_derive_for_data(
        name,
        SpaceDataRef::Primary,
        kind,
        input,
        options,
        output_schema,
        span,
    )
}

fn stat_derive_for_data(
    name: String,
    data: SpaceDataRef,
    kind: StatKind,
    input: FrameIr,
    options: StatOptionsIr,
    output_schema: Vec<ColumnDefIr>,
    span: Span,
) -> DeriveIr {
    DeriveIr {
        name,
        data,
        stat: StatCallIr {
            kind,
            input,
            options,
            span,
        },
        output_schema,
        span,
    }
}

fn derived_space(
    name: String,
    frame: FrameIr,
    geometries: Vec<GeometryIr>,
    theme: Option<ThemeIr>,
    guides: GuideOverridesIr,
    scales: Vec<ScaleIr>,
    span: Span,
) -> SpaceIr {
    let layers = geometries
        .iter()
        .cloned()
        .map(SpaceLayerIr::Geometry)
        .collect();
    SpaceIr {
        data: SpaceDataRef::Derived(name),
        frame,
        layers,
        geometries,
        guides,
        scales,
        theme,
        projection: None,
        // Inherits the parent space's coordinate system (set in `space()`).
        coords: CoordsIr::Cartesian,
        view: CoordinateViewIr::default(),
        span,
    }
}

fn with_annotations(rect: GeometryIr, annotations: Vec<GeometryIr>) -> Vec<GeometryIr> {
    let mut geometries = vec![rect];
    geometries.extend(annotations);
    geometries
}

/// Visual settings copied verbatim from a high-level geometry onto the
/// low-level mark it lowers into (fill area / rect / line).
const FILL_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Alpha,
];

/// Settings passed through to a grouped histogram's `Rect`, where `fill` is a
/// group-column mapping rather than a literal color (spec §15.6).
const GROUPED_RECT_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Alpha,
];
const STROKE_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Dash,
    PropertyKey::Alpha,
];

const INTERVAL_SEGMENT_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Dash,
    PropertyKey::Alpha,
];
const INTERVAL_SEGMENT_MAPPINGS: &[PropertyKey] = &[PropertyKey::Stroke, PropertyKey::Alpha];
const POINT_RANGE_POINT_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::Alpha,
    PropertyKey::Size,
    PropertyKey::Shape,
];
const POINT_RANGE_POINT_MAPPINGS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::Alpha,
    PropertyKey::Size,
    PropertyKey::Shape,
];
const CROSS_BAR_RECT_SETTINGS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Alpha,
];
const CROSS_BAR_RECT_MAPPINGS: &[PropertyKey] =
    &[PropertyKey::Fill, PropertyKey::Stroke, PropertyKey::Alpha];

struct IntervalParts {
    orientation: IntervalOrientationIr,
    position: ColumnRef,
    lower: ColumnRef,
    upper: ColumnRef,
}

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

fn passthrough_mappings(geometry: &GeometryIr, allow: &[PropertyKey]) -> Vec<AestheticMapping> {
    geometry
        .mappings
        .iter()
        .filter(|mapping| allow.contains(&mapping.aesthetic))
        .cloned()
        .collect()
}

fn mapping_column(geometry: &GeometryIr, key: PropertyKey) -> Option<ColumnRef> {
    geometry
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == key)
        .map(|mapping| mapping.column.clone())
}

fn number_setting_from_geometry(geometry: &GeometryIr, key: PropertyKey) -> Option<f64> {
    geometry
        .settings
        .iter()
        .find_map(|setting| match &setting.value {
            SettingValue::Number(n) if setting.name == key && n.is_finite() && *n >= 0.0 => {
                Some(*n)
            }
            _ => None,
        })
}

fn segment_endpoint_mappings(
    span: Span,
    x_dtype: DataType,
    y_dtype: DataType,
) -> Vec<AestheticMapping> {
    vec![
        mapping(PropertyKey::X, "x", x_dtype, span),
        mapping(PropertyKey::Y, "y", y_dtype, span),
        mapping(PropertyKey::Xend, "xend", x_dtype, span),
        mapping(PropertyKey::Yend, "yend", y_dtype, span),
    ]
}

fn mapping(aesthetic: PropertyKey, name: &str, dtype: DataType, span: Span) -> AestheticMapping {
    AestheticMapping {
        aesthetic,
        column: synthetic_column(name, dtype, span),
        span,
    }
}

fn schema_dtype(schema: &[ColumnDefIr], name: &str) -> DataType {
    schema
        .iter()
        .find(|column| column.name == name)
        .map(|column| column.dtype)
        .unwrap_or(DataType::Float)
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
