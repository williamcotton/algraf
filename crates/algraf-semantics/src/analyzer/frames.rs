//! Space and algebraic-frame analysis (spec §8, §13.3, §13.5, §13.17 phases
//! 8–12): space data binding, projection, frame construction, column
//! resolution, and structural frame checks.

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{parse_temporal_literal, DataType};
use algraf_syntax::ast::{
    AlgebraBinary, AlgebraCall, AlgebraExpr, AlgebraName, AlgebraOp, Arg, InsetBlock, InsetItem,
    LetDecl, LiteralKind, SpaceBlock, SpaceItem, ValueExpr,
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
        self.space_with_default(space, None)
    }

    fn space_with_default(
        &mut self,
        space: &SpaceBlock,
        default_data: Option<(SpaceDataRef, ActiveTable)>,
    ) -> SpaceAnalysis {
        let span = node_span(space.syntax());
        let (data_ref, table) = self.space_data(space, default_data);

        let previous_row_context = self.row_context_tables.clone();
        let previous_space_vars = self.space_vars.clone();
        let mut row_context = Vec::with_capacity(previous_row_context.len() + 1);
        row_context.push(table.clone());
        row_context.extend(previous_row_context.iter().cloned());
        self.row_context_tables = row_context;

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
        let mut scope_vars = previous_space_vars.clone();
        scope_vars.extend(self.collect_let_decls(&space_lets));
        self.space_vars = scope_vars;

        let frame_expr = space.frame();
        let frame = match frame_expr.as_ref() {
            Some(expr) => {
                let frame = self.build_frame(expr, &table);
                self.check_cartesian_arity(&frame, node_span(expr.syntax()));
                self.check_facet_variable(&frame);
                self.check_temporal_nesting(&frame);
                frame
            }
            None => FrameIr::Invalid,
        };
        let projection = self.space_projection(space);
        let coords = self.space_coords(space, &frame, projection.is_some());
        let view = self.space_view(space, &coords, projection.is_some());

        let mut geometry_layers = Vec::new();
        let mut source_layers = Vec::new();
        let mut inset_derived = Vec::new();
        let mut theme: Option<ThemeIr> = None;
        let mut guides = GuideOverridesIr::default();
        let mut scales = Vec::new();
        let mut saw_layer = false;
        for item in space.items() {
            match item {
                SpaceItem::Geometry(call) => {
                    saw_layer = true;
                    if let Some(geo) = self.geometry(&call, &frame, &coords, &table) {
                        source_layers.push(SpaceLayerIr::Geometry(geo.clone()));
                        geometry_layers.push(geo);
                    }
                }
                SpaceItem::Inset(block) => {
                    saw_layer = true;
                    if let Some(inset) = self.inset(&block, &table, &mut inset_derived) {
                        source_layers.push(SpaceLayerIr::Inset(inset));
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
        if !saw_layer {
            self.diag(Diagnostic::warning(codes::W2001, "empty Space block", span));
        }

        let primitive_geometries: Vec<GeometryIr> = geometry_layers
            .iter()
            .filter(|geo| !is_lowered_geometry(geo))
            .cloned()
            .collect();
        self.check_spatial_geometries(&primitive_geometries, &frame, projection.is_some());

        let histogram_count = geometry_layers
            .iter()
            .filter(|geo| geo.kind == GeometryKind::Histogram)
            .count();
        let histogram_annotation_mode = histogram_count == 1
            && geometry_layers
                .iter()
                .all(|geo| geo.kind == GeometryKind::Histogram || is_histogram_annotation(geo));
        let histogram_annotations = if histogram_annotation_mode {
            geometry_layers
                .iter()
                .filter(|geo| geo.kind != GeometryKind::Histogram)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        let mut analysis = SpaceAnalysis::default();
        analysis.derived.extend(inset_derived);

        if source_layers
            .iter()
            .any(|layer| matches!(layer, SpaceLayerIr::Inset(_)))
        {
            for geo in &geometry_layers {
                if is_lowered_geometry(geo) {
                    self.diag(
                        Diagnostic::error(
                            codes::E2101,
                            "high-level geometry lowering inside a space with `Inset` is not supported",
                            geo.span,
                        )
                        .with_help("move the derived table into an explicit `Derive` and render a primitive mark"),
                    );
                }
            }
            analysis.spaces.push(SpaceIr {
                data: data_ref,
                frame,
                layers: source_layers,
                geometries: primitive_geometries,
                guides,
                scales,
                theme,
                projection,
                coords,
                view,
                span,
            });
            self.space_vars = previous_space_vars;
            self.row_context_tables = previous_row_context;
            return analysis;
        }

        let mut pending = Vec::new();
        for geo in geometry_layers {
            if histogram_annotation_mode && geo.kind != GeometryKind::Histogram {
                continue;
            }
            match geo.kind {
                GeometryKind::Histogram => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derive, histogram_space)) = self.desugar_histogram(
                        &geo,
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
                GeometryKind::FreqPoly => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derive, freq_space)) = self.desugar_freq_poly(
                        &geo,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                    ) {
                        analysis.derived.push(derive);
                        analysis.spaces.push(freq_space);
                    }
                }
                GeometryKind::Bin2D => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derive, bin2d_space)) = self.desugar_bin2d(
                        &geo,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                    ) {
                        analysis.derived.push(derive);
                        analysis.spaces.push(bin2d_space);
                    }
                }
                GeometryKind::Density => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derive, density_space)) = self.desugar_density(
                        &geo,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                    ) {
                        analysis.derived.push(derive);
                        analysis.spaces.push(density_space);
                    }
                }
                GeometryKind::Bar if has_count_stat(&geo) => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derive, count_space)) = self.desugar_count_bar(
                        &geo,
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
                _ if is_interval_sugar(&geo) => {
                    push_pending_space(
                        &mut analysis,
                        &mut pending,
                        &data_ref,
                        &frame,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                        projection.clone(),
                        view,
                        span,
                    );
                    if let Some((derives, spaces)) = self.desugar_interval_sugar(
                        &geo,
                        &frame,
                        &data_ref,
                        theme.clone(),
                        guides.clone(),
                        scales.clone(),
                    ) {
                        analysis.derived.extend(derives);
                        analysis.spaces.extend(spaces);
                    }
                }
                _ => pending.push(geo),
            }
        }
        push_pending_space(
            &mut analysis,
            &mut pending,
            &data_ref,
            &frame,
            theme.clone(),
            guides.clone(),
            scales.clone(),
            projection.clone(),
            view,
            span,
        );
        if analysis.spaces.is_empty() {
            analysis.spaces.push(SpaceIr {
                data: data_ref,
                frame,
                layers: Vec::new(),
                geometries: Vec::new(),
                guides,
                scales,
                theme,
                projection,
                coords,
                view,
                span,
            });
        }
        // Desugared spaces (histogram/freq-poly/bin2d/density/count-bar) inherit
        // the parent space's coordinate system, so a polar `Histogram` yields a
        // circular histogram (spec §16.16).
        for produced in &mut analysis.spaces {
            produced.coords = coords;
            produced.view = view;
        }
        // Space-scope bindings do not leak into sibling spaces (spec §9.6).
        self.space_vars = previous_space_vars;
        self.row_context_tables = previous_row_context;
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

    /// Read Cartesian coordinate-view controls (`zoomX`, `zoomY`, and
    /// `aspect`). They affect rendering after stat materialization and are
    /// ignored for polar or spatial spaces.
    fn space_view(
        &mut self,
        space: &SpaceBlock,
        coords: &CoordsIr,
        has_projection: bool,
    ) -> CoordinateViewIr {
        if matches!(coords, CoordsIr::Polar { .. }) || has_projection {
            return CoordinateViewIr::default();
        }
        let args = space.args();
        CoordinateViewIr {
            zoom_x: self.space_zoom_arg(&args, "zoomX"),
            zoom_y: self.space_zoom_arg(&args, "zoomY"),
            aspect: self.space_aspect_arg(&args),
        }
    }

    fn space_zoom_arg(&mut self, args: &[Arg], key: &str) -> Option<AxisViewDomainIr> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some(key))?;
        let value = arg.value()?;
        let Some(bounds) = self.view_bounds(&value) else {
            self.diag(Diagnostic::error(
                codes::E1204,
                format!("`{key}` expects [min, max] with numbers, temporal literals, or null"),
                node_span(value.syntax()),
            ));
            return None;
        };
        Some(AxisViewDomainIr {
            min: bounds[0],
            max: bounds[1],
        })
    }

    fn space_aspect_arg(&mut self, args: &[Arg]) -> Option<f64> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some("aspect"))?;
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                let value = lit
                    .text()
                    .and_then(|t| t.parse::<f64>().ok())
                    .unwrap_or(0.0);
                if value.is_finite() && value > 0.0 {
                    Some(value)
                } else {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`aspect` expects a positive finite number",
                        node_span(lit.syntax()),
                    ));
                    None
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`aspect` expects a positive finite number",
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn view_bounds(&mut self, value: &ValueExpr) -> Option<[Option<f64>; 2]> {
        let ValueExpr::Array(array) = value else {
            return None;
        };
        let elems = array.values();
        if elems.len() != 2 {
            return None;
        }
        let mut out = [None, None];
        for (index, elem) in elems.iter().enumerate() {
            match elem {
                ValueExpr::Literal(lit) => match lit.kind() {
                    Some(LiteralKind::Number) => {
                        let n = lit.text().and_then(|t| t.parse::<f64>().ok())?;
                        if !n.is_finite() {
                            return None;
                        }
                        out[index] = Some(n);
                    }
                    Some(LiteralKind::Null) => out[index] = None,
                    _ => return None,
                },
                ValueExpr::Call(call) => {
                    out[index] = Some(self.temporal_view_bound(call)? as f64);
                }
                _ => return None,
            }
        }
        Some(out)
    }

    fn temporal_view_bound(&mut self, call: &algraf_syntax::ast::CallValue) -> Option<i64> {
        let name = call.name().unwrap_or_default();
        let require_date = match name.as_str() {
            "date" => true,
            "datetime" => false,
            _ => return None,
        };
        let span = node_span(call.syntax());
        let args = call.args();
        let text = match args.first() {
            Some(arg) if args.len() == 1 && arg.key().is_none() => match arg.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1017,
                        format!("`{name}(...)` expects a single quoted temporal string"),
                        span,
                    ));
                    return None;
                }
            },
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1017,
                    format!("`{name}(...)` expects a single quoted temporal string"),
                    span,
                ));
                return None;
            }
        };
        match parse_temporal_literal(&text, require_date) {
            Some(micros) => Some(micros),
            None => {
                self.diag(Diagnostic::error(
                    codes::E1017,
                    format!(
                        "{text:?} is not a recognized {} literal",
                        if require_date { "date" } else { "datetime" }
                    ),
                    span,
                ));
                None
            }
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

    fn inset(
        &mut self,
        block: &InsetBlock,
        parent_table: &ActiveTable,
        derived_out: &mut Vec<DeriveIr>,
    ) -> Option<InsetIr> {
        let span = node_span(block.syntax());
        let args = block.args();
        let mut data_ref = None;
        let mut child_table = None;
        let mut match_rules = Vec::new();
        let mut width = None;
        let mut height = None;
        let mut size_number = None;
        let mut size_column = None;
        let mut min_size = 16.0;
        let mut max_size = 48.0;
        let mut scale_policy = InsetScalePolicyIr::Shared;
        let mut guides = false;
        let mut clip = InsetClipIr::Rect;
        let mut padding = 2.0;
        let mut placement = InsetPlacementIr::Center;
        let mut dx = 0.0;
        let mut dy = 0.0;
        let mut anchor = InsetAnchorIr::Position;

        for arg in &args {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            match key.as_str() {
                "data" => {
                    if let Some((data, table)) = self.inset_data_ref(arg) {
                        data_ref = Some(data);
                        child_table = Some(table);
                    }
                }
                "match" => {
                    if let Some(table) = child_table.as_ref() {
                        match_rules = self.inset_match_rules(arg, table);
                    }
                }
                "width" => width = self.inset_number_arg(arg, "`width`"),
                "height" => height = self.inset_number_arg(arg, "`height`"),
                "size" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                        size_number = self.inset_number_arg(arg, "`size`");
                    }
                    Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                        size_column = Some(self.resolve_column(&name, parent_table));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E2106,
                        "`size` expects a number or a numeric column in the parent table",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "minSize" => {
                    if let Some(value) = self.inset_number_arg(arg, "`minSize`") {
                        min_size = value;
                    }
                }
                "maxSize" => {
                    if let Some(value) = self.inset_number_arg(arg, "`maxSize`") {
                        max_size = value;
                    }
                }
                "scales" => {
                    if let Some(value) = self.inset_string_arg(arg, "`scales`") {
                        match value.as_str() {
                            "shared" => scale_policy = InsetScalePolicyIr::Shared,
                            "local" => scale_policy = InsetScalePolicyIr::Local,
                            _ => self.diag(Diagnostic::error(
                                codes::E2101,
                                "`scales` must be \"shared\" or \"local\"",
                                key_span,
                            )),
                        }
                    }
                }
                "guides" => {
                    if let Some(value) = self.inset_bool_arg(arg, "`guides`") {
                        guides = value;
                    }
                }
                "clip" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        match string_value(&lit.text().unwrap_or_default()).as_str() {
                            "rect" => clip = InsetClipIr::Rect,
                            "circle" => clip = InsetClipIr::Circle,
                            other => self.diag(Diagnostic::error(
                                codes::E2101,
                                format!(
                                    "`clip` must be \"rect\", \"circle\", or false, not {other:?}"
                                ),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                        if lit.text().as_deref() == Some("false") {
                            clip = InsetClipIr::None;
                        } else {
                            self.diag(Diagnostic::error(
                                codes::E2101,
                                "`clip` must be \"rect\", \"circle\", or false",
                                node_span(lit.syntax()),
                            ));
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E2101,
                        "`clip` must be \"rect\", \"circle\", or false",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "padding" => {
                    if let Some(value) = self.inset_number_arg(arg, "`padding`") {
                        padding = value.max(0.0);
                    }
                }
                "placement" => {
                    if let Some(value) = self.inset_string_arg(arg, "`placement`") {
                        match value.as_str() {
                            "center" => placement = InsetPlacementIr::Center,
                            "mark-center" => placement = InsetPlacementIr::MarkCenter,
                            _ => self.diag(Diagnostic::error(
                                codes::E2101,
                                "`placement` must be \"center\" or \"mark-center\"",
                                key_span,
                            )),
                        }
                    }
                }
                "dx" => {
                    if let Some(value) = self.inset_number_arg(arg, "`dx`") {
                        dx = value;
                    }
                }
                "dy" => {
                    if let Some(value) = self.inset_number_arg(arg, "`dy`") {
                        dy = value;
                    }
                }
                "anchor" => {
                    if let Some(value) = self.inset_string_arg(arg, "`anchor`") {
                        match value.as_str() {
                            "position" => anchor = InsetAnchorIr::Position,
                            "centroid" => anchor = InsetAnchorIr::Centroid,
                            _ => self.diag(Diagnostic::error(
                                codes::E2105,
                                "`anchor` must be \"position\" or \"centroid\"",
                                key_span,
                            )),
                        }
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E2101,
                    format!("unsupported Inset argument `{key}`"),
                    key_span,
                )),
            }
        }

        if match_rules.is_empty() {
            if let (Some(table), Some(match_arg)) = (
                child_table.as_ref(),
                args.iter()
                    .find(|arg| arg.key().as_deref() == Some("match")),
            ) {
                match_rules = self.inset_match_rules(match_arg, table);
            }
        }

        if child_table.is_some()
            && match_rules.is_empty()
            && !args.iter().any(|arg| arg.key().as_deref() == Some("match"))
        {
            self.diag(
                Diagnostic::error(codes::E2103, "`Inset` requires an explicit `match:`", span)
                    .with_help("write an equi-match such as `match: [city => city]`"),
            );
        }

        let Some(data_ref) = data_ref else {
            self.diag(Diagnostic::error(
                codes::E2102,
                "`Inset` requires `data:` naming a chart table or derived table",
                span,
            ));
            return None;
        };
        let child_table = child_table.unwrap_or_else(|| ActiveTable {
            columns: Vec::new(),
        });

        if (width.is_some() || height.is_some()) && (size_number.is_some() || size_column.is_some())
        {
            self.diag(Diagnostic::error(
                codes::E2106,
                "`Inset` cannot combine `size` with `width` or `height`",
                span,
            ));
        }
        if min_size > max_size {
            std::mem::swap(&mut min_size, &mut max_size);
        }
        let size = match size_column {
            Some(column) => InsetSizeIr::Mapped {
                column,
                min: min_size.max(0.0),
                max: max_size.max(0.0),
            },
            None => {
                let w = width.or(size_number).unwrap_or(32.0).max(0.0);
                let h = height.or(size_number).unwrap_or(w).max(0.0);
                InsetSizeIr::Fixed {
                    width: w,
                    height: h,
                }
            }
        };

        let mut inset_theme: Option<ThemeIr> = None;
        let mut inset_guides = GuideOverridesIr::default();
        let mut inset_scales = Vec::new();
        let inset_lets = block
            .items()
            .into_iter()
            .filter_map(|item| match item {
                InsetItem::Let(decl) => Some(decl),
                _ => None,
            })
            .collect::<Vec<_>>();
        for item in block.items() {
            match item {
                InsetItem::Theme(decl) => {
                    if let Some(theme) = self.theme_decl(&decl) {
                        inset_theme = Some(theme);
                    }
                }
                InsetItem::Scale(decl) => {
                    if let Some(scale) = self.scale_decl(&decl, &child_table) {
                        inset_scales.push(scale);
                    }
                }
                InsetItem::Guide(decl) => self.guide_decl(&decl, &mut inset_guides),
                InsetItem::Space(_) | InsetItem::Let(_) | InsetItem::Error(_) => {}
            }
        }

        let previous_space_vars = self.space_vars.clone();
        let mut inset_scope_vars = previous_space_vars.clone();
        inset_scope_vars.extend(self.collect_let_decls(&inset_lets));
        self.space_vars = inset_scope_vars;

        let mut child_spaces = Vec::new();
        for item in block.items() {
            if let InsetItem::Space(space) = item {
                let mut analysis =
                    self.space_with_default(&space, Some((data_ref.clone(), child_table.clone())));
                derived_out.extend(analysis.derived);
                for child in &mut analysis.spaces {
                    if child.theme.is_none() {
                        child.theme = inset_theme.clone();
                    }
                    child.guides = merge_guide_overrides(&inset_guides, &child.guides);
                    if !inset_scales.is_empty() {
                        let mut scales = inset_scales.clone();
                        scales.extend(child.scales.clone());
                        child.scales = scales;
                    }
                }
                child_spaces.extend(analysis.spaces);
            }
        }
        self.space_vars = previous_space_vars;
        if child_spaces.is_empty() {
            self.diag(Diagnostic::warning(
                codes::W2001,
                "`Inset` block contains no child Space",
                span,
            ));
        }

        Some(InsetIr {
            data: data_ref,
            match_rules,
            size,
            scale_policy,
            guides,
            clip,
            padding,
            placement,
            dx,
            dy,
            anchor,
            child_spaces,
            span,
        })
    }

    fn inset_data_ref(&mut self, arg: &Arg) -> Option<(SpaceDataRef, ActiveTable)> {
        match arg.value() {
            Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) if name.qualifier().is_none() => {
                let table_name = name.name().unwrap_or_default();
                if let Some(schema) = self.derived.get(&table_name) {
                    return Some((
                        SpaceDataRef::Derived(table_name),
                        ActiveTable::from_ir(schema),
                    ));
                }
                if self.table_names.contains(&table_name) {
                    let table = self.table_active(&table_name);
                    return Some((SpaceDataRef::Table(table_name), table));
                }
                self.diag(Diagnostic::error(
                    codes::E2102,
                    format!("unknown inset child table `{table_name}`"),
                    node_span(name.syntax()),
                ));
                None
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E2102,
                    "`Inset(data:)` must name a chart table or derived table",
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn inset_match_rules(&mut self, arg: &Arg, child_table: &ActiveTable) -> Vec<InsetMatchIr> {
        let Some(value) = arg.value() else {
            return Vec::new();
        };
        let ValueExpr::Map(map) = value else {
            self.diag(Diagnostic::error(
                codes::E2103,
                "`match` expects a map such as `[child_key => parent_key]`",
                node_span(value.syntax()),
            ));
            return Vec::new();
        };
        let mut rules = Vec::new();
        for entry in map.entries() {
            let span = node_span(entry.syntax());
            let Some(child_expr) = entry.key() else {
                continue;
            };
            let Some(parent_expr) = entry.value() else {
                continue;
            };
            let Some(child) = self.inset_child_match_column(&child_expr, child_table) else {
                continue;
            };
            let Some(parent) = self.inset_parent_match_column(&parent_expr) else {
                continue;
            };
            if !match_types_compatible(child.dtype, parent.column().dtype) {
                self.diag(Diagnostic::error(
                    codes::E2105,
                    format!(
                        "inset match compares `{}` ({:?}) with `{}` ({:?})",
                        child.name,
                        child.dtype,
                        parent.column().name,
                        parent.column().dtype
                    ),
                    span,
                ));
            }
            rules.push(InsetMatchIr {
                child,
                parent,
                span,
            });
        }
        rules
    }

    fn inset_child_match_column(
        &mut self,
        value: &ValueExpr,
        child_table: &ActiveTable,
    ) -> Option<ColumnRef> {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(name)) if name.qualifier().is_none() => {
                Some(self.resolve_column(name, child_table))
            }
            _ => {
                self.diag(Diagnostic::error(
                    codes::E2103,
                    "left side of an inset match must be a child-table column",
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    fn inset_parent_match_column(&mut self, value: &ValueExpr) -> Option<InsetParentRefIr> {
        let ValueExpr::Algebra(AlgebraExpr::Name(name)) = value else {
            self.diag(Diagnostic::error(
                codes::E2104,
                "right side of an inset match must be a current or parent row column",
                node_span(value.syntax()),
            ));
            return None;
        };
        let qualifier = name.qualifier();
        let table_index = match qualifier.as_deref() {
            None => 0,
            Some("parent") => 1,
            Some(other) => {
                self.diag(Diagnostic::error(
                    codes::E2104,
                    format!("unknown row-context qualifier `{other}`"),
                    node_span(name.syntax()),
                ));
                return None;
            }
        };
        let Some(table) = self.row_context_tables.get(table_index).cloned() else {
            self.diag(Diagnostic::error(
                codes::E2104,
                "`parent.` reference has no parent row context here",
                node_span(name.syntax()),
            ));
            return None;
        };
        let column = if qualifier.is_some() {
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
                    self.diag(Diagnostic::error(
                        codes::E1101,
                        format!("unknown parent row column `{col_name}`"),
                        span,
                    ));
                    ColumnRef {
                        name: col_name,
                        dtype: DataType::Unknown,
                        span,
                    }
                }
            }
        } else {
            self.resolve_column(name, &table)
        };
        if qualifier.is_some() {
            Some(InsetParentRefIr::Parent(column))
        } else {
            Some(InsetParentRefIr::Current(column))
        }
    }

    fn inset_number_arg(&mut self, arg: &Arg, label: &str) -> Option<f64> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                let value = lit.text().and_then(|text| text.parse::<f64>().ok())?;
                if value.is_finite() {
                    Some(value)
                } else {
                    self.diag(Diagnostic::error(
                        codes::E2106,
                        format!("{label} expects a finite number"),
                        node_span(lit.syntax()),
                    ));
                    None
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E2106,
                    format!("{label} expects a number"),
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn inset_string_arg(&mut self, arg: &Arg, label: &str) -> Option<String> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E2101,
                    format!("{label} expects a string literal"),
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn inset_bool_arg(&mut self, arg: &Arg, label: &str) -> Option<bool> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                Some(lit.text().as_deref() == Some("true"))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E2101,
                    format!("{label} expects a boolean"),
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn space_data(
        &mut self,
        space: &SpaceBlock,
        default_data: Option<(SpaceDataRef, ActiveTable)>,
    ) -> (SpaceDataRef, ActiveTable) {
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

        default_data.unwrap_or_else(|| {
            (
                SpaceDataRef::Primary,
                ActiveTable::from_schema(self.primary),
            )
        })
    }

    // --- Algebra frame (spec §8, §13.5) ---

    fn build_frame(&mut self, expr: &AlgebraExpr, table: &ActiveTable) -> FrameIr {
        match expr {
            AlgebraExpr::Name(name) => FrameIr::Vector(self.resolve_column(name, table)),
            AlgebraExpr::Call(call) => self.build_frame_call(call),
            AlgebraExpr::Paren(paren) => match paren.inner() {
                Some(inner) => self.build_frame(&inner, table),
                None => FrameIr::Invalid,
            },
            AlgebraExpr::Binary(binary) => self.build_binary(binary, table),
            AlgebraExpr::Error(_) => FrameIr::Invalid,
        }
    }

    fn build_frame_call(&mut self, call: &AlgebraCall) -> FrameIr {
        let name = call.name().unwrap_or_default();
        let name_span = call.name_span().unwrap_or_else(|| node_span(call.syntax()));
        if name == "transpose" {
            let mut diagnostic = Diagnostic::error(
                codes::E1912,
                "`transpose(...)` was removed; write physical x/y frame order directly",
                name_span,
            );
            if let Some(replacement) = call.inner().and_then(|inner| transpose_replacement(&inner))
            {
                diagnostic = diagnostic.with_help(format!(
                    "frame order is physical; write `{replacement}` instead"
                ));
            }
            self.diag(diagnostic);
            return FrameIr::Invalid;
        }

        self.diag(Diagnostic::error(
            codes::E1912,
            format!("unsupported frame operator `{name}`"),
            name_span,
        ));
        FrameIr::Invalid
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
        if let Some(qualifier) = name.qualifier() {
            self.diag(
                Diagnostic::error(
                    codes::E2104,
                    format!("qualified row reference `{qualifier}.{col_name}` is only valid in `Inset(match:)`"),
                    node_span(name.syntax()),
                )
                .with_help("use `parent.column` only on the right side of an inset match rule"),
            );
            return ColumnRef {
                name: col_name,
                dtype: DataType::Unknown,
                span,
            };
        }
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

fn transpose_replacement(expr: &AlgebraExpr) -> Option<String> {
    match expr {
        AlgebraExpr::Paren(paren) => {
            let inner = paren.inner()?;
            transpose_replacement(&inner)
        }
        AlgebraExpr::Binary(binary) if binary.op() == Some(AlgebraOp::Cross) => {
            let lhs = binary.lhs()?;
            let rhs = binary.rhs()?;
            Some(format!(
                "{} * {}",
                algebra_text_wrapped(&rhs),
                algebra_text_wrapped(&lhs)
            ))
        }
        AlgebraExpr::Binary(binary) if binary.op() == Some(AlgebraOp::Nest) => {
            let outer = binary.lhs()?;
            let inner = binary.rhs()?;
            let swapped = transpose_replacement(&outer)?;
            Some(format!("({swapped}) / {}", algebra_text_wrapped(&inner)))
        }
        _ => None,
    }
}

fn algebra_text_wrapped(expr: &AlgebraExpr) -> String {
    let text = expr.syntax().text().to_string().trim().to_string();
    match expr {
        AlgebraExpr::Binary(_) => format!("({text})"),
        _ => text,
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

fn is_interval_sugar(geo: &GeometryIr) -> bool {
    matches!(
        geo.kind,
        GeometryKind::ErrorBar
            | GeometryKind::LineRange
            | GeometryKind::PointRange
            | GeometryKind::CrossBar
    )
}

fn is_lowered_geometry(geo: &GeometryIr) -> bool {
    matches!(
        geo.kind,
        GeometryKind::Histogram
            | GeometryKind::FreqPoly
            | GeometryKind::Bin2D
            | GeometryKind::Density
    ) || has_count_stat(geo)
        || is_interval_sugar(geo)
}

fn merge_guide_overrides(
    inherited: &GuideOverridesIr,
    local: &GuideOverridesIr,
) -> GuideOverridesIr {
    GuideOverridesIr {
        legend: local.legend.or(inherited.legend),
        fill_legend: local.fill_legend.or(inherited.fill_legend),
        stroke_legend: local.stroke_legend.or(inherited.stroke_legend),
        grid: local.grid.or(inherited.grid),
        x_label: local.x_label.clone().or_else(|| inherited.x_label.clone()),
        y_label: local.y_label.clone().or_else(|| inherited.y_label.clone()),
        x_time_format: local
            .x_time_format
            .clone()
            .or_else(|| inherited.x_time_format.clone()),
        y_time_format: local
            .y_time_format
            .clone()
            .or_else(|| inherited.y_time_format.clone()),
        x_tick_label_angle: local.x_tick_label_angle.or(inherited.x_tick_label_angle),
        y_tick_label_angle: local.y_tick_label_angle.or(inherited.y_tick_label_angle),
        x_tick_label_rows: local.x_tick_label_rows.or(inherited.x_tick_label_rows),
        y_tick_label_rows: local.y_tick_label_rows.or(inherited.y_tick_label_rows),
        grid_shape: local.grid_shape.or(inherited.grid_shape),
    }
}

fn match_types_compatible(left: DataType, right: DataType) -> bool {
    left == DataType::Unknown
        || right == DataType::Unknown
        || left == right
        || matches!(
            (left, right),
            (DataType::Integer, DataType::Float) | (DataType::Float, DataType::Integer)
        )
}

#[allow(clippy::too_many_arguments)]
fn push_pending_space(
    analysis: &mut SpaceAnalysis,
    pending: &mut Vec<GeometryIr>,
    data_ref: &SpaceDataRef,
    frame: &FrameIr,
    theme: Option<ThemeIr>,
    guides: GuideOverridesIr,
    scales: Vec<ScaleIr>,
    projection: Option<String>,
    view: CoordinateViewIr,
    span: Span,
) {
    if pending.is_empty() {
        return;
    }
    let geometries = std::mem::take(pending);
    let layers = geometries
        .iter()
        .cloned()
        .map(SpaceLayerIr::Geometry)
        .collect();
    analysis.spaces.push(SpaceIr {
        data: data_ref.clone(),
        frame: frame.clone(),
        layers,
        geometries,
        guides,
        scales,
        theme,
        projection,
        coords: CoordsIr::Cartesian,
        view,
        span,
    });
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
