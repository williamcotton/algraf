//! Space and algebraic-frame analysis (spec §8, §13.3, §13.5, §13.17 phases
//! 8–12): space data binding, projection, frame construction, column
//! resolution, and structural frame checks.

use std::collections::HashMap;

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::DataType;
use algraf_syntax::ast::{
    AlgebraBinary, AlgebraExpr, AlgebraName, AlgebraOp, Arg, LetDecl, LiteralKind, SpaceBlock,
    SpaceItem, ValueExpr,
};
use algraf_syntax::{node_span, unescape_string_literal as string_value, SyntaxKind};

use super::context::{ActiveTable, Analyzer};
use crate::ir::*;
use crate::util::closest;

#[derive(Default)]
pub(super) struct SpaceAnalysis {
    pub(super) derived: Vec<DeriveIr>,
    pub(super) spaces: Vec<SpaceIr>,
}

impl Analyzer<'_> {
    // --- Space (spec §13.3, §13.17 phases 8–12) ---

    pub(super) fn space(&mut self, space: &SpaceBlock) -> SpaceAnalysis {
        let span = node_span(space.syntax());
        let (data_ref, table) = self.space_data(space);

        // Collect space-scope `let` bindings; these shadow chart-scope bindings
        // of the same name for the duration of this space (spec §9.6).
        let space_lets: Vec<LetDecl> = space
            .items()
            .into_iter()
            .filter_map(|item| match item {
                SpaceItem::Let(decl) => Some(decl),
                _ => None,
            })
            .collect();
        self.space_vars = self.collect_let_decls(&space_lets);

        let frame = match space.frame() {
            Some(expr) => {
                let frame = self.build_frame(&expr, &table);
                self.check_cartesian_arity(&frame, node_span(expr.syntax()));
                self.check_facet_variable(&frame);
                self.check_temporal_nesting(&frame);
                frame
            }
            None => FrameIr::Invalid,
        };
        let projection = self.space_projection(space);
        let coords = self.space_coords(space, &frame, projection.is_some());

        let mut geometries = Vec::new();
        let mut histograms = Vec::new();
        let mut freq_polys = Vec::new();
        let mut bin2ds = Vec::new();
        let mut densities = Vec::new();
        let mut count_bars = Vec::new();
        let mut theme: Option<ThemeIr> = None;
        let mut guides = GuideOverridesIr::default();
        let mut scales = Vec::new();
        let mut saw_geometry = false;
        for item in space.items() {
            match item {
                SpaceItem::Geometry(call) => {
                    saw_geometry = true;
                    if let Some(geo) = self.geometry(&call, &frame, &coords, &table) {
                        if geo.kind == GeometryKind::Histogram {
                            histograms.push(geo);
                        } else if geo.kind == GeometryKind::FreqPoly {
                            freq_polys.push(geo);
                        } else if geo.kind == GeometryKind::Bin2D {
                            bin2ds.push(geo);
                        } else if geo.kind == GeometryKind::Density {
                            densities.push(geo);
                        } else if geo.kind == GeometryKind::Bar && has_count_stat(&geo) {
                            count_bars.push(geo);
                        } else {
                            geometries.push(geo);
                        }
                    }
                }
                SpaceItem::Theme(decl) => {
                    if let Some(t) = self.theme_decl(&decl) {
                        theme = Some(t);
                    }
                }
                SpaceItem::Scale(decl) => {
                    if let Some(scale) = self.scale_decl(&decl, &table) {
                        scales.push(scale);
                    }
                }
                SpaceItem::Guide(decl) => self.guide_decl(&decl, &mut guides),
                SpaceItem::Let(_) => {}
                SpaceItem::Error(_) => {}
            }
        }
        if !saw_geometry {
            self.diag(Diagnostic::warning(codes::W2001, "empty Space block", span));
        }
        self.check_spatial_geometries(&geometries, &frame, projection.is_some());
        let histogram_annotations = if histograms.len() == 1
            && freq_polys.is_empty()
            && bin2ds.is_empty()
            && densities.is_empty()
            && count_bars.is_empty()
            && geometries.iter().all(is_histogram_annotation)
        {
            std::mem::take(&mut geometries)
        } else {
            Vec::new()
        };

        let mut analysis = SpaceAnalysis::default();
        for histogram in histograms {
            if let Some((derive, histogram_space)) = self.desugar_histogram(
                &histogram,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
                histogram_annotations.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(histogram_space);
            }
        }
        for freq_poly in freq_polys {
            if let Some((derive, freq_space)) = self.desugar_freq_poly(
                &freq_poly,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(freq_space);
            }
        }
        for bin2d in bin2ds {
            if let Some((derive, bin2d_space)) = self.desugar_bin2d(
                &bin2d,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(bin2d_space);
            }
        }
        for density in densities {
            if let Some((derive, density_space)) = self.desugar_density(
                &density,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(density_space);
            }
        }
        for bar in count_bars {
            if let Some((derive, count_space)) = self.desugar_count_bar(
                &bar,
                &frame,
                &data_ref,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(count_space);
            }
        }
        if !geometries.is_empty() || analysis.spaces.is_empty() {
            analysis.spaces.push(SpaceIr {
                data: data_ref,
                frame,
                geometries,
                guides,
                scales,
                theme,
                projection,
                coords,
                span,
            });
        }
        // Desugared spaces (histogram/freq-poly/bin2d/density/count-bar) inherit
        // the parent space's coordinate system, so a polar `Histogram` yields a
        // circular histogram (spec §16.16).
        for produced in &mut analysis.spaces {
            produced.coords = coords;
        }
        // Space-scope bindings do not leak into sibling spaces (spec §9.6).
        self.space_vars = HashMap::new();
        analysis
    }

    /// Read the optional `projection:` argument of a space as a string literal
    /// (spec §16.14). The string's validity (alias or PROJ form) is checked at
    /// render time, where the projection registry lives (`E1802`).
    fn space_projection(&mut self, space: &SpaceBlock) -> Option<String> {
        let arg = space
            .args()
            .into_iter()
            .find(|a| a.key().as_deref() == Some("projection"))?;
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1802,
                    "`projection` expects a string literal (an alias or a `+proj=…` string)",
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    /// Read and validate the polar coordinate arguments of a space (spec §4.2,
    /// §16.16): `coords` (`"cartesian"` default | `"polar"`), `theta` (`"x"`
    /// default | `"y"`), and `innerRadius` (a fraction in `[0, 1)`). Cartesian is
    /// returned for any non-polar or invalid configuration so rendering is
    /// unaffected. A spatial (projected) space ignores `coords` — combining polar
    /// with geographic projections is deferred.
    fn space_coords(
        &mut self,
        space: &SpaceBlock,
        frame: &FrameIr,
        has_projection: bool,
    ) -> CoordsIr {
        let args = space.args();
        let Some(coords_arg) = args.iter().find(|a| a.key().as_deref() == Some("coords")) else {
            return CoordsIr::Cartesian;
        };
        let (coords_value, value_span) = match coords_arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => (
                string_value(&lit.text().unwrap_or_default()),
                node_span(lit.syntax()),
            ),
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1901,
                    "`coords` expects a string literal: \"cartesian\" or \"polar\"",
                    node_span(value.syntax()),
                ));
                return CoordsIr::Cartesian;
            }
            None => return CoordsIr::Cartesian,
        };
        match coords_value.as_str() {
            "cartesian" => CoordsIr::Cartesian,
            "polar" => {
                if has_projection {
                    // Polar + geographic projection is deferred (spec §16.15);
                    // the projection wins and the space stays spatial.
                    return CoordsIr::Cartesian;
                }
                let theta = self.polar_theta(&args);
                let inner_radius = self.polar_inner_radius(&args);
                let start_angle = self.polar_start_angle(&args);
                let direction = self.polar_direction(&args);
                // The transform supports 1D and 2D (a * b) frames. Faceted
                // (nested) polar frames are deferred.
                match frame {
                    FrameIr::Nested { .. } => {
                        self.diag(Diagnostic::error(
                            codes::E1904,
                            "polar coordinates support a 1D or 2D (a * b) frame, not a faceted frame",
                            value_span,
                        ));
                        CoordsIr::Cartesian
                    }
                    FrameIr::Invalid => CoordsIr::Cartesian,
                    _ => CoordsIr::Polar {
                        theta,
                        inner_radius,
                        start_angle,
                        direction,
                    },
                }
            }
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1901,
                    format!("unknown coordinate system {coords_value:?}; expected \"cartesian\" or \"polar\""),
                    value_span,
                ));
                CoordsIr::Cartesian
            }
        }
    }

    /// Read the `theta:` argument (`"x"` default | `"y"`), selecting which frame
    /// axis maps to the angle under a polar transform (spec §16.16).
    fn polar_theta(&mut self, args: &[Arg]) -> PolarThetaIr {
        let Some(arg) = args.iter().find(|a| a.key().as_deref() == Some("theta")) else {
            return PolarThetaIr::X;
        };
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                match string_value(&lit.text().unwrap_or_default()).as_str() {
                    "x" => PolarThetaIr::X,
                    "y" => PolarThetaIr::Y,
                    other => {
                        self.diag(Diagnostic::error(
                            codes::E1902,
                            format!("`theta` must be \"x\" or \"y\", not {other:?}"),
                            node_span(lit.syntax()),
                        ));
                        PolarThetaIr::X
                    }
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1902,
                    "`theta` expects a string literal: \"x\" or \"y\"",
                    node_span(value.syntax()),
                ));
                PolarThetaIr::X
            }
            None => PolarThetaIr::X,
        }
    }

    /// Read the `innerRadius:` argument: a numeric literal in `[0, 1)` (a fraction
    /// of the maximum radius; `0` = pie, `> 0` = donut, spec §16.16).
    fn polar_inner_radius(&mut self, args: &[Arg]) -> f64 {
        let Some(arg) = args
            .iter()
            .find(|a| a.key().as_deref() == Some("innerRadius"))
        else {
            return 0.0;
        };
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                match lit.text().and_then(|t| t.parse::<f64>().ok()) {
                    Some(value) if (0.0..1.0).contains(&value) => value,
                    _ => {
                        self.diag(Diagnostic::error(
                            codes::E1903,
                            "`innerRadius` must be a number in [0, 1)",
                            node_span(lit.syntax()),
                        ));
                        0.0
                    }
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1903,
                    "`innerRadius` expects a numeric literal in [0, 1)",
                    node_span(value.syntax()),
                ));
                0.0
            }
            None => 0.0,
        }
    }

    /// Read the `startAngle:` argument: a finite numeric literal in degrees,
    /// clockwise from 12 o'clock, placing the theta-domain minimum. The default
    /// `0` reproduces the fixed 12-o'clock origin of earlier versions (spec
    /// §16.16). Accepts the full `[-360, 360]` range so any orientation is
    /// expressible.
    fn polar_start_angle(&mut self, args: &[Arg]) -> f64 {
        let Some(arg) = args
            .iter()
            .find(|a| a.key().as_deref() == Some("startAngle"))
        else {
            return 0.0;
        };
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                match lit.text().and_then(|t| t.parse::<f64>().ok()) {
                    Some(value) if value.is_finite() && (-360.0..=360.0).contains(&value) => value,
                    _ => {
                        self.diag(Diagnostic::error(
                            codes::E1909,
                            "`startAngle` must be a finite number of degrees in [-360, 360]",
                            node_span(lit.syntax()),
                        ));
                        0.0
                    }
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1909,
                    "`startAngle` expects a numeric literal in degrees",
                    node_span(value.syntax()),
                ));
                0.0
            }
            None => 0.0,
        }
    }

    /// Read the `direction:` argument (`"clockwise"` default |
    /// `"counterclockwise"`), selecting the angular sweep sense (spec §16.16).
    fn polar_direction(&mut self, args: &[Arg]) -> PolarDirectionIr {
        let Some(arg) = args
            .iter()
            .find(|a| a.key().as_deref() == Some("direction"))
        else {
            return PolarDirectionIr::Clockwise;
        };
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                match string_value(&lit.text().unwrap_or_default()).as_str() {
                    "clockwise" => PolarDirectionIr::Clockwise,
                    "counterclockwise" => PolarDirectionIr::CounterClockwise,
                    other => {
                        self.diag(Diagnostic::error(
                            codes::E1910,
                            format!(
                                "`direction` must be \"clockwise\" or \"counterclockwise\", not {other:?}"
                            ),
                            node_span(lit.syntax()),
                        ));
                        PolarDirectionIr::Clockwise
                    }
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1910,
                    "`direction` expects a string literal: \"clockwise\" or \"counterclockwise\"",
                    node_span(value.syntax()),
                ));
                PolarDirectionIr::Clockwise
            }
            None => PolarDirectionIr::Clockwise,
        }
    }

    /// Validate `Geo` marks against their space frame (spec §16.14, §14.x). A
    /// `Geo` mark requires a spatial space: its frame must be a single geometry
    /// column. A single non-geometry column is `E1801`; a planar (multi-axis)
    /// frame is `E1804`.
    fn check_spatial_geometries(
        &mut self,
        geometries: &[GeometryIr],
        frame: &FrameIr,
        has_projection: bool,
    ) {
        for geo in geometries {
            match geo.kind {
                GeometryKind::Geo => self.check_geo_mark(frame, geo.span),
                GeometryKind::Graticule => {
                    self.check_graticule_mark(frame, has_projection, geo.span)
                }
                _ => {}
            }
        }
    }

    /// A `Geo` mark requires a single geometry column (spec §14.23).
    fn check_geo_mark(&mut self, frame: &FrameIr, span: algraf_core::Span) {
        match frame {
            FrameIr::Vector(col) if col.dtype == DataType::Geometry => {}
            // The column was already reported as unknown (E1101); avoid a
            // confusing second diagnostic.
            FrameIr::Vector(col) if col.dtype == DataType::Unknown => {}
            FrameIr::Vector(_) => self.diag(Diagnostic::error(
                codes::E1801,
                "a spatial space requires a geometry column; \
                 `Geo` must be used in a `Space(geom)` over a geometry column",
                span,
            )),
            FrameIr::Invalid => {}
            _ => self.diag(Diagnostic::error(
                codes::E1804,
                "`Geo` mark requires a spatial space (a `Space` over a geometry column), \
                 not a planar Cartesian space",
                span,
            )),
        }
    }

    /// A `Graticule` mark requires a spatial space: a geometry-column frame, or a
    /// planar frame with a declared `projection:` (spec §14.24).
    fn check_graticule_mark(
        &mut self,
        frame: &FrameIr,
        has_projection: bool,
        span: algraf_core::Span,
    ) {
        let spatial = has_projection
            || matches!(frame, FrameIr::Vector(col) if col.dtype == DataType::Geometry);
        // Unknown columns / invalid frames already produced their own diagnostic.
        let suppressed = matches!(frame, FrameIr::Invalid)
            || matches!(frame, FrameIr::Vector(col) if col.dtype == DataType::Unknown);
        if !spatial && !suppressed {
            self.diag(Diagnostic::error(
                codes::E1804,
                "`Graticule` mark requires a spatial space (a geometry column \
                 or a space with a declared `projection:`)",
                span,
            ));
        }
    }

    fn space_data(&mut self, space: &SpaceBlock) -> (SpaceDataRef, ActiveTable) {
        let data_arg = space
            .args()
            .into_iter()
            .find(|a| a.key().as_deref() == Some("data"));

        if let Some(arg) = data_arg {
            if let Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) = arg.value() {
                let table_name = name.name().unwrap_or_default();
                if let Some(schema) = self.derived.get(&table_name) {
                    return (
                        SpaceDataRef::Derived(table_name),
                        ActiveTable::from_ir(schema),
                    );
                }
                if self.table_names.contains(&table_name) {
                    let table = self.table_active(&table_name);
                    return (SpaceDataRef::Table(table_name), table);
                }
                self.diag(Diagnostic::error(
                    codes::E1103,
                    format!("unknown table `{table_name}`"),
                    node_span(name.syntax()),
                ));
            } else if let Some(value) = arg.value() {
                self.diag(Diagnostic::error(
                    codes::E1103,
                    "space `data` must name a derived or declared table",
                    node_span(value.syntax()),
                ));
            }
        }

        (
            SpaceDataRef::Primary,
            ActiveTable::from_schema(self.primary),
        )
    }

    // --- Algebra frame (spec §8, §13.5) ---

    fn build_frame(&mut self, expr: &AlgebraExpr, table: &ActiveTable) -> FrameIr {
        match expr {
            AlgebraExpr::Name(name) => FrameIr::Vector(self.resolve_column(name, table)),
            AlgebraExpr::Paren(paren) => match paren.inner() {
                Some(inner) => self.build_frame(&inner, table),
                None => FrameIr::Invalid,
            },
            AlgebraExpr::Binary(binary) => self.build_binary(binary, table),
            AlgebraExpr::Error(_) => FrameIr::Invalid,
        }
    }

    fn build_binary(&mut self, binary: &AlgebraBinary, table: &ActiveTable) -> FrameIr {
        let lhs = binary
            .lhs()
            .map(|e| self.build_frame(&e, table))
            .unwrap_or(FrameIr::Invalid);
        let rhs = binary
            .rhs()
            .map(|e| self.build_frame(&e, table))
            .unwrap_or(FrameIr::Invalid);

        match binary.op() {
            Some(AlgebraOp::Cross) => cartesian_push(lhs, rhs),
            Some(AlgebraOp::Nest) => FrameIr::Nested {
                outer: Box::new(lhs),
                inner: Box::new(rhs),
            },
            Some(AlgebraOp::Blend) => {
                if !blend_parenthesized(binary) {
                    self.diag(
                        Diagnostic::error(
                            codes::E1305,
                            "blend `+` expression must be parenthesized",
                            node_span(binary.syntax()),
                        )
                        .with_help("wrap it in parentheses, e.g. `time * (lower + upper)`"),
                    );
                }
                union_push(lhs, rhs)
            }
            None => FrameIr::Invalid,
        }
    }

    pub(super) fn resolve_column(&mut self, name: &AlgebraName, table: &ActiveTable) -> ColumnRef {
        let col_name = name.name().unwrap_or_default();
        let span = name
            .ident_span()
            .unwrap_or_else(|| node_span(name.syntax()));
        match table.get(&col_name) {
            Some(dtype) => ColumnRef {
                name: col_name,
                dtype,
                span,
            },
            None => {
                let mut diag =
                    Diagnostic::error(codes::E1101, format!("unknown column `{col_name}`"), span);
                if let Some(suggestion) = closest(&col_name, table.names()) {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
                ColumnRef {
                    name: col_name,
                    dtype: DataType::Unknown,
                    span,
                }
            }
        }
    }

    /// Reject 3D-or-higher Cartesian spaces (spec §8.3, §13.14).
    fn check_cartesian_arity(&mut self, frame: &FrameIr, span: Span) {
        match frame {
            FrameIr::Cartesian(axes) => {
                if axes.len() > 2 {
                    self.diag(
                        Diagnostic::error(
                            codes::E1306,
                            "3D Cartesian spaces are unsupported",
                            span,
                        )
                        .with_help("use nesting to facet, e.g. `(x * y) / z`"),
                    );
                }
                for axis in axes {
                    self.check_cartesian_arity(axis, span);
                }
            }
            FrameIr::Nested { outer, inner } => {
                self.check_cartesian_arity(outer, span);
                self.check_cartesian_arity(inner, span);
            }
            FrameIr::Union(members) => {
                for m in members {
                    self.check_cartesian_arity(m, span);
                }
            }
            FrameIr::Vector(_) | FrameIr::Invalid => {}
        }
    }

    fn check_facet_variable(&mut self, frame: &FrameIr) {
        if let Some(panel) = facet_panel_column(frame) {
            if panel.dtype != DataType::Unknown && !panel.dtype.is_categorical() {
                self.diag(
                    Diagnostic::error(
                        codes::E1303,
                        format!("facet column `{}` must be categorical", panel.name),
                        panel.span,
                    )
                    .with_help("use a string, boolean, or pre-binned column for facet panels"),
                );
            }
        }
    }

    fn check_temporal_nesting(&mut self, frame: &FrameIr) {
        match frame {
            FrameIr::Nested { outer, inner } => {
                if direct_temporal_vector(outer) || direct_temporal_vector(inner) {
                    self.diag(
                        Diagnostic::warning(
                            codes::W2008,
                            "high-cardinality temporal nesting may create excessive bands or panels",
                            temporal_nesting_span(outer)
                                .or_else(|| temporal_nesting_span(inner))
                                .unwrap_or(Span::new(0, 0)),
                        )
                        .with_help(
                            "precompute a coarser period column such as day, week, month, or year",
                        ),
                    );
                }
                self.check_temporal_nesting(outer);
                self.check_temporal_nesting(inner);
            }
            FrameIr::Cartesian(axes) | FrameIr::Union(axes) => {
                for axis in axes {
                    self.check_temporal_nesting(axis);
                }
            }
            FrameIr::Vector(_) | FrameIr::Invalid => {}
        }
    }
}

/// Whether a blend `+` node is acceptably parenthesized (spec §8.5).
///
/// A blend node is valid if its parent is a parenthesized expression, or if it
/// is an inner link of a blend chain whose root is parenthesized.
fn blend_parenthesized(binary: &AlgebraBinary) -> bool {
    match binary.syntax().parent() {
        Some(parent) if parent.kind() == SyntaxKind::ALGEBRA_PAREN => true,
        Some(parent) if parent.kind() == SyntaxKind::ALGEBRA_BINARY => {
            AlgebraBinary::cast(parent).and_then(|b| b.op()) == Some(AlgebraOp::Blend)
        }
        _ => false,
    }
}

fn has_count_stat(geo: &GeometryIr) -> bool {
    geo.settings.iter().any(|setting| {
        setting.name == PropertyKey::Stat
            && matches!(&setting.value, SettingValue::String(v) if v == "count")
    })
}

fn is_histogram_annotation(geo: &GeometryIr) -> bool {
    matches!(
        geo.kind,
        GeometryKind::HLine | GeometryKind::VLine | GeometryKind::Text | GeometryKind::Segment
    )
}

pub(super) fn contains_nested(frame: &FrameIr) -> bool {
    match frame {
        FrameIr::Nested { .. } => true,
        FrameIr::Cartesian(members) | FrameIr::Union(members) => {
            members.iter().any(contains_nested)
        }
        FrameIr::Vector(_) | FrameIr::Invalid => false,
    }
}

fn facet_panel_column(frame: &FrameIr) -> Option<&ColumnRef> {
    let FrameIr::Nested { outer, inner } = frame else {
        return None;
    };
    if !matches!(outer.as_ref(), FrameIr::Cartesian(axes) if axes.len() == 2) {
        return None;
    }
    match inner.as_ref() {
        FrameIr::Vector(column) => Some(column),
        _ => None,
    }
}

fn direct_temporal_vector(frame: &FrameIr) -> bool {
    matches!(frame, FrameIr::Vector(column) if column.dtype == DataType::Temporal)
}

fn temporal_nesting_span(frame: &FrameIr) -> Option<Span> {
    match frame {
        FrameIr::Vector(column) if column.dtype == DataType::Temporal => Some(column.span),
        _ => None,
    }
}

fn cartesian_push(acc: FrameIr, next: FrameIr) -> FrameIr {
    match acc {
        FrameIr::Cartesian(mut axes) => {
            axes.push(next);
            FrameIr::Cartesian(axes)
        }
        other => FrameIr::Cartesian(vec![other, next]),
    }
}

fn union_push(acc: FrameIr, next: FrameIr) -> FrameIr {
    match acc {
        FrameIr::Union(mut members) => {
            members.push(next);
            FrameIr::Union(members)
        }
        other => FrameIr::Union(vec![other, next]),
    }
}
