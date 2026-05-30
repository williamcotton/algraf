//! Explicit `Derive` stat analysis (spec §13.4): dependency ordering, stat
//! input arity, setting validation, and derived output schemas.

use std::collections::{HashMap, HashSet};

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::DataType;
use algraf_syntax::ast::{AlgebraExpr, Arg, ChartBlock, ChartItem, DeriveDecl};
use algraf_syntax::node_span;

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer, ValueForm};
use crate::ir::*;
use crate::planning::{stat_output_names_for_source, stat_output_schema};

impl Analyzer<'_> {
    // --- Derive (spec §13.4) ---

    pub(super) fn resolve_chart_derives(&mut self, chart: &ChartBlock) -> Vec<DeriveIr> {
        let primary_table = ActiveTable::from_schema(self.primary);
        let mut decls = Vec::new();
        let mut seen_names: HashMap<String, Span> = HashMap::new();

        for item in chart.items() {
            let ChartItem::Derive(derive) = item else {
                continue;
            };
            let span = node_span(derive.syntax());
            let Some(name) = derive.name() else { continue };
            if let Some(&first) = seen_names.get(&name) {
                self.diag(
                    Diagnostic::error(
                        codes::E1104,
                        format!("duplicate derived table `{name}`"),
                        span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen_names.insert(name.clone(), span);
            decls.push((name, derive));
        }

        let mut producer_by_column: HashMap<String, usize> = HashMap::new();
        for (index, (_, derive)) in decls.iter().enumerate() {
            for output in derive_output_names(derive) {
                producer_by_column.entry(output).or_insert(index);
            }
        }

        let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); decls.len()];
        for (index, (_, derive)) in decls.iter().enumerate() {
            for input in derive_input_names(derive) {
                if primary_table.get(&input).is_some() {
                    continue;
                }
                if let Some(&producer) = producer_by_column.get(&input) {
                    deps[index].insert(producer);
                }
            }
        }

        let mut resolved = HashSet::new();
        let mut pending: HashSet<usize> = (0..decls.len()).collect();
        let mut out = Vec::new();
        let mut schemas: HashMap<usize, Vec<ColumnDefIr>> = HashMap::new();

        while !pending.is_empty() {
            let mut ready: Vec<usize> = pending
                .iter()
                .copied()
                .filter(|index| deps[*index].iter().all(|dep| resolved.contains(dep)))
                .collect();
            ready.sort_unstable();

            if ready.is_empty() {
                for index in pending.iter().copied() {
                    let (_, derive) = &decls[index];
                    self.diag(Diagnostic::error(
                        codes::E1501,
                        "cycle between derived table declarations",
                        node_span(derive.syntax()),
                    ));
                }
                break;
            }

            for index in ready {
                pending.remove(&index);
                let mut upstream: Vec<usize> = deps[index].iter().copied().collect();
                upstream.sort_unstable();
                let data = if upstream.is_empty() {
                    SpaceDataRef::Primary
                } else if upstream.len() == 1 {
                    SpaceDataRef::Derived(decls[upstream[0]].0.clone())
                } else {
                    self.diag(Diagnostic::error(
                        codes::E1404,
                        "derived stat inputs must come from one upstream table",
                        node_span(decls[index].1.syntax()),
                    ));
                    SpaceDataRef::Derived(decls[upstream[0]].0.clone())
                };
                let upstream_schemas: Vec<&[ColumnDefIr]> = upstream
                    .iter()
                    .filter_map(|dep| schemas.get(dep).map(Vec::as_slice))
                    .collect();
                let table = ActiveTable::merged(self.primary, &upstream_schemas);
                if let Some(ir) = self.derive(&decls[index].1, &table, data) {
                    schemas.insert(index, ir.output_schema.clone());
                    resolved.insert(index);
                    out.push(ir);
                }
            }
        }

        out
    }

    fn derive(
        &mut self,
        derive: &DeriveDecl,
        table: &ActiveTable,
        data: SpaceDataRef,
    ) -> Option<DeriveIr> {
        let span = node_span(derive.syntax());
        let name = derive.name()?;

        let stat = derive.stat()?;
        let stat_name = stat.name().unwrap_or_default();
        let stat_span = node_span(stat.syntax());
        let kind = match stat_name.as_str() {
            "Bin" => StatKind::Bin,
            "Smooth" => StatKind::Smooth,
            "StepVertices" => StatKind::StepVertices,
            "VectorEndpoints" => StatKind::VectorEndpoints,
            "CurveSample" => StatKind::CurveSample,
            "IntervalSegments" => StatKind::IntervalSegments,
            "IntervalRects" => StatKind::IntervalRects,
            "IntervalMiddles" => StatKind::IntervalMiddles,
            "Bin2D" => StatKind::Bin2D,
            "HexBin" => StatKind::HexBin,
            "Centroid" => StatKind::Centroid,
            "Simplify" => StatKind::Simplify,
            "SpatialJoin" => StatKind::SpatialJoin,
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1403,
                    format!("unknown stat `{stat_name}`; supported stats are `Bin`, `Smooth`, `StepVertices`, `VectorEndpoints`, `CurveSample`, `IntervalSegments`, `IntervalRects`, `IntervalMiddles`, `Bin2D`, `HexBin`, `Centroid`, `Simplify`, and `SpatialJoin`"),
                    stat_span,
                ));
                return None;
            }
        };

        let inputs = stat.inputs();
        let (input_frame, options, output_schema) = match kind {
            StatKind::Bin => {
                let input_frame = self.single_stat_input(&inputs, table, stat_span, "Bin")?;
                if let FrameIr::Vector(col) = &input_frame {
                    match col.dtype {
                        DataType::Temporal
                        | DataType::Integer
                        | DataType::Float
                        | DataType::Unknown => {}
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            format!("Bin input column `{}` is not numeric or temporal", col.name),
                            col.span,
                        )),
                    }
                }
                let input_dtype = match &input_frame {
                    FrameIr::Vector(col) => Some(col.dtype),
                    _ => None,
                };
                let options = self.collect_bin_options(&stat.args(), stat_span, input_dtype);
                let output_schema = stat_output_schema(kind, &input_frame);
                (input_frame, options, output_schema)
            }
            StatKind::Smooth => {
                let input_frame = self.two_stat_inputs(&inputs, table, stat_span, "Smooth")?;
                if let FrameIr::Cartesian(columns) = &input_frame {
                    for frame in columns {
                        if let FrameIr::Vector(col) = frame {
                            if !matches!(
                                col.dtype,
                                DataType::Integer | DataType::Float | DataType::Unknown
                            ) {
                                self.diag(Diagnostic::error(
                                    codes::E1404,
                                    format!("Smooth input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                let options = self.collect_smooth_options(&stat.args(), stat_span);
                let se = matches!(&options, StatOptionsIr::Smooth { se: true, .. });
                let output_schema = crate::planning::smooth_output_schema(se);
                (input_frame, options, output_schema)
            }
            StatKind::StepVertices => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "StepVertices", 2)?;
                if let FrameIr::Cartesian(columns) = &input_frame {
                    for frame in columns {
                        if let FrameIr::Vector(col) = frame {
                            if matches!(col.dtype, DataType::Geometry) {
                                self.diag(Diagnostic::error(
                                    codes::E1404,
                                    format!(
                                        "StepVertices input column `{}` is a geometry column",
                                        col.name
                                    ),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                let options = self.collect_step_vertices_options(&stat.args());
                let output_schema = stat_output_schema(kind, &input_frame);
                (input_frame, options, output_schema)
            }
            StatKind::VectorEndpoints => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "VectorEndpoints", 4)?;
                self.require_numeric_stat_inputs(&input_frame, "VectorEndpoints");
                let options = self.collect_vector_endpoints_options(&stat.args());
                let output_schema = primitive_output_schema(
                    crate::planning::vector_endpoints_output_schema(),
                    table,
                    &["x", "y", "xend", "yend"],
                );
                (input_frame, options, output_schema)
            }
            StatKind::CurveSample => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "CurveSample", 4)?;
                self.require_numeric_stat_inputs(&input_frame, "CurveSample");
                let options = self.collect_curve_sample_options(&stat.args());
                let output_schema = primitive_output_schema(
                    crate::planning::curve_sample_output_schema(),
                    table,
                    &["x", "y", "link_id"],
                );
                (input_frame, options, output_schema)
            }
            StatKind::IntervalSegments => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "IntervalSegments", 3)?;
                self.reject_geometry_stat_inputs(&input_frame, "IntervalSegments");
                let (orientation, cap_width) =
                    self.collect_interval_segment_options(&stat.args(), stat_span);
                let output_schema = primitive_output_schema(
                    crate::planning::interval_segments_output_schema(&input_frame, orientation),
                    table,
                    &["x", "y", "xend", "yend", "interval_role", "interval_id"],
                );
                (
                    input_frame,
                    StatOptionsIr::IntervalSegments {
                        orientation,
                        cap_width,
                    },
                    output_schema,
                )
            }
            StatKind::IntervalRects => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "IntervalRects", 3)?;
                self.reject_geometry_stat_inputs(&input_frame, "IntervalRects");
                let (orientation, width) =
                    self.collect_interval_width_options(&stat.args(), stat_span, "IntervalRects");
                let output_schema = primitive_output_schema(
                    crate::planning::interval_rects_output_schema(&input_frame, orientation),
                    table,
                    &[
                        "xmin",
                        "xmax",
                        "ymin",
                        "ymax",
                        "interval_role",
                        "interval_id",
                    ],
                );
                (
                    input_frame,
                    StatOptionsIr::IntervalRects { orientation, width },
                    output_schema,
                )
            }
            StatKind::IntervalMiddles => {
                let input_frame =
                    self.n_stat_inputs(&inputs, table, stat_span, "IntervalMiddles", 2)?;
                self.reject_geometry_stat_inputs(&input_frame, "IntervalMiddles");
                let (orientation, width) =
                    self.collect_interval_width_options(&stat.args(), stat_span, "IntervalMiddles");
                let output_schema = primitive_output_schema(
                    crate::planning::interval_middles_output_schema(&input_frame, orientation),
                    table,
                    &["x", "y", "xend", "yend", "interval_role", "interval_id"],
                );
                (
                    input_frame,
                    StatOptionsIr::IntervalMiddles { orientation, width },
                    output_schema,
                )
            }
            StatKind::Bin2D | StatKind::HexBin => {
                let label = if kind == StatKind::Bin2D {
                    "Bin2D"
                } else {
                    "HexBin"
                };
                let input_frame = self.two_stat_inputs(&inputs, table, stat_span, label)?;
                if let FrameIr::Cartesian(columns) = &input_frame {
                    for frame in columns {
                        if let FrameIr::Vector(col) = frame {
                            if !matches!(
                                col.dtype,
                                DataType::Integer | DataType::Float | DataType::Unknown
                            ) {
                                self.diag(Diagnostic::error(
                                    codes::E1404,
                                    format!("{label} input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                let options = self.collect_bin2d_options(&stat.args(), stat_span, kind);
                let output_schema = stat_output_schema(kind, &input_frame);
                (input_frame, options, output_schema)
            }
            StatKind::Centroid => {
                let input_frame =
                    self.single_geometry_input(&inputs, table, stat_span, "Centroid")?;
                self.reject_stat_args(&stat.args(), "Centroid");
                let output_schema = geometry_stat_output_schema(table);
                (input_frame, StatOptionsIr::Centroid, output_schema)
            }
            StatKind::Simplify => {
                let input_frame =
                    self.single_geometry_input(&inputs, table, stat_span, "Simplify")?;
                let options = self.collect_simplify_options(&stat.args());
                let output_schema = geometry_stat_output_schema(table);
                (input_frame, options, output_schema)
            }
            StatKind::SpatialJoin => {
                let input_frame =
                    self.single_geometry_input(&inputs, table, stat_span, "SpatialJoin")?;
                let (options, output_schema) =
                    self.spatial_join_options(&stat.args(), table, stat_span)?;
                (input_frame, options, output_schema)
            }
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1403,
                    format!("unsupported stat `{stat_name}`"),
                    stat_span,
                ));
                return None;
            }
        };

        Some(DeriveIr {
            name,
            data,
            stat: StatCallIr {
                kind,
                input: input_frame,
                options,
                span: stat_span,
            },
            output_schema,
            span,
        })
    }

    fn single_stat_input(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
    ) -> Option<FrameIr> {
        if inputs.len() != 1 {
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("{stat_name} requires exactly one input column"),
                stat_span,
            ));
            return None;
        }
        match &inputs[0] {
            AlgebraExpr::Name(n) => Some(FrameIr::Vector(self.resolve_column(n, table))),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("{stat_name} requires a column input"),
                    stat_span,
                ));
                Some(FrameIr::Invalid)
            }
        }
    }

    /// A single geometry-column stat input (`Centroid`/`Simplify`). Reports a
    /// non-geometry input as `E1404`.
    fn single_geometry_input(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
    ) -> Option<FrameIr> {
        let frame = self.single_stat_input(inputs, table, stat_span, stat_name)?;
        if let FrameIr::Vector(col) = &frame {
            if !matches!(col.dtype, DataType::Geometry | DataType::Unknown) {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!(
                        "{stat_name} input column `{}` is not a geometry column",
                        col.name
                    ),
                    col.span,
                ));
            }
        }
        Some(frame)
    }

    /// Reject any arguments on a stat that takes none (e.g. `Centroid`).
    fn reject_stat_args(&mut self, args: &[Arg], stat_name: &str) {
        for arg in args {
            if let Some(name) = arg.key() {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("{stat_name} takes no settings; unknown setting `{name}`"),
                    node_span(arg.syntax()),
                ));
            }
        }
    }

    /// Parse `Simplify(geom, tolerance: n)` settings. `tolerance` must be a
    /// non-negative finite number.
    fn collect_simplify_options(&mut self, args: &[Arg]) -> StatOptionsIr {
        let mut tolerance = None;
        let mut dup = DupGuard::new(codes::E1404, "Simplify setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "tolerance" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() && n >= 0.0 => tolerance = Some(n),
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`tolerance` expects a non-negative number",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown Simplify setting `{name}`"),
                    key_span,
                )),
            }
        }
        StatOptionsIr::Simplify { tolerance }
    }

    /// Parse and validate `SpatialJoin(geom, table: "name", predicate: "within")`
    /// (spec §15.14), returning its options and the joined output schema. The
    /// `table:` argument MUST name a chart-scoped `Table` with a geometry column.
    fn spatial_join_options(
        &mut self,
        args: &[Arg],
        point_table: &ActiveTable,
        stat_span: Span,
    ) -> Option<(StatOptionsIr, Vec<ColumnDefIr>)> {
        let mut table_name = None;
        let mut predicate = SpatialPredicateIr::Within;
        let mut dup = DupGuard::new(codes::E1404, "SpatialJoin setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "table" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        // The polygon table is named by a bare identifier, like a
                        // `Space(data: name)` reference.
                        ValueForm::Column(column) => table_name = column.name(),
                        ValueForm::Str(s) => table_name = Some(s),
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`table` expects a chart-scoped table name",
                            node_span(value.syntax()),
                        )),
                    }
                }
                "predicate" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "within" => {
                            predicate = SpatialPredicateIr::Within
                        }
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`predicate` expects \"within\" (the only supported predicate)",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown SpatialJoin setting `{name}`"),
                    key_span,
                )),
            }
        }

        let Some(table_name) = table_name else {
            self.diag(Diagnostic::error(
                codes::E1404,
                "SpatialJoin requires a `table:` naming the polygon table to join against",
                stat_span,
            ));
            return None;
        };
        if !self.table_names.contains(&table_name) {
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("SpatialJoin `table` `{table_name}` is not a chart-scoped `Table`"),
                stat_span,
            ));
            return None;
        }
        let polygon = self.table_active(&table_name);
        let polygon_pairs: Vec<(String, DataType)> = polygon.columns.clone();
        if !polygon_pairs.iter().any(|(_, d)| *d == DataType::Geometry) {
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("SpatialJoin target table `{table_name}` has no geometry column"),
                stat_span,
            ));
            return None;
        }

        // Output = point-side columns, then appended polygon scalar columns.
        let mut output = geometry_stat_output_schema(point_table);
        let point_names: Vec<&str> = point_table.names().collect();
        let appended = crate::planning::spatial_join_appended_columns(
            point_names,
            polygon_pairs.iter().map(|(n, d)| (n.as_str(), *d)),
        );
        output.extend(appended);

        Some((
            StatOptionsIr::SpatialJoin {
                table: table_name,
                predicate,
            },
            output,
        ))
    }

    fn two_stat_inputs(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
    ) -> Option<FrameIr> {
        if inputs.len() != 2 {
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("{stat_name} requires exactly two input columns"),
                stat_span,
            ));
            return None;
        }
        let mut frames = Vec::new();
        for input in inputs {
            match input {
                AlgebraExpr::Name(n) => frames.push(FrameIr::Vector(self.resolve_column(n, table))),
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1404,
                        format!("{stat_name} requires column inputs"),
                        stat_span,
                    ));
                    frames.push(FrameIr::Invalid);
                }
            }
        }
        Some(FrameIr::Cartesian(frames))
    }

    fn n_stat_inputs(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
        count: usize,
    ) -> Option<FrameIr> {
        if inputs.len() != count {
            self.diag(Diagnostic::error(
                codes::E1404,
                format!("{stat_name} requires exactly {count} input columns"),
                stat_span,
            ));
            return None;
        }
        let mut frames = Vec::with_capacity(count);
        for input in inputs {
            match input {
                AlgebraExpr::Name(n) => frames.push(FrameIr::Vector(self.resolve_column(n, table))),
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1404,
                        format!("{stat_name} requires column inputs"),
                        stat_span,
                    ));
                    frames.push(FrameIr::Invalid);
                }
            }
        }
        Some(FrameIr::Cartesian(frames))
    }

    fn require_numeric_stat_inputs(&mut self, frame: &FrameIr, stat_name: &str) {
        if let FrameIr::Cartesian(columns) = frame {
            for frame in columns {
                if let FrameIr::Vector(col) = frame {
                    if !matches!(
                        col.dtype,
                        DataType::Integer | DataType::Float | DataType::Unknown
                    ) {
                        self.diag(Diagnostic::error(
                            codes::E1404,
                            format!("{stat_name} input column `{}` is not numeric", col.name),
                            col.span,
                        ));
                    }
                }
            }
        }
    }

    fn reject_geometry_stat_inputs(&mut self, frame: &FrameIr, stat_name: &str) {
        if let FrameIr::Cartesian(columns) = frame {
            for frame in columns {
                if let FrameIr::Vector(col) = frame {
                    if matches!(col.dtype, DataType::Geometry) {
                        self.diag(Diagnostic::error(
                            codes::E1404,
                            format!(
                                "{stat_name} input column `{}` is a geometry column",
                                col.name
                            ),
                            col.span,
                        ));
                    }
                }
            }
        }
    }

    fn collect_interval_segment_options(
        &mut self,
        args: &[Arg],
        stat_span: Span,
    ) -> (IntervalOrientationIr, Option<f64>) {
        let mut orientation = IntervalOrientationIr::Vertical;
        let mut cap_width = None;
        let mut dup = DupGuard::new(codes::E1404, "IntervalSegments setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "orientation" => {
                    orientation = self.interval_orientation(arg, "IntervalSegments");
                }
                "capWidth" => {
                    cap_width = self.non_negative_number_option(arg, "capWidth");
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown IntervalSegments setting `{name}`"),
                    key_span,
                )),
            }
        }
        let _ = stat_span;
        (orientation, cap_width)
    }

    fn collect_interval_width_options(
        &mut self,
        args: &[Arg],
        stat_span: Span,
        stat_name: &str,
    ) -> (IntervalOrientationIr, Option<f64>) {
        let mut orientation = IntervalOrientationIr::Vertical;
        let mut width = None;
        let mut dup = DupGuard::new(codes::E1404, "interval setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "orientation" => {
                    orientation = self.interval_orientation(arg, stat_name);
                }
                "width" => {
                    width = self.non_negative_number_option(arg, "width");
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown {stat_name} setting `{name}`"),
                    key_span,
                )),
            }
        }
        let _ = stat_span;
        (orientation, width)
    }

    fn interval_orientation(&mut self, arg: &Arg, stat_name: &str) -> IntervalOrientationIr {
        let Some(value) = arg.value() else {
            return IntervalOrientationIr::Vertical;
        };
        match ValueForm::of(&value) {
            ValueForm::Str(s) if s == "vertical" => IntervalOrientationIr::Vertical,
            ValueForm::Str(s) if s == "horizontal" => IntervalOrientationIr::Horizontal,
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("{stat_name} `orientation` expects \"vertical\" or \"horizontal\""),
                    node_span(value.syntax()),
                ));
                IntervalOrientationIr::Vertical
            }
        }
    }

    fn non_negative_number_option(&mut self, arg: &Arg, name: &str) -> Option<f64> {
        let value = arg.value()?;
        match ValueForm::of(&value) {
            ValueForm::Number(n) if n.is_finite() && n >= 0.0 => Some(n),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("`{name}` expects a non-negative finite number"),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    fn collect_step_vertices_options(&mut self, args: &[Arg]) -> StatOptionsIr {
        let mut direction = StepDirectionIr::Hv;
        let mut dup = DupGuard::new(codes::E1404, "StepVertices setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "direction" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "hv" => direction = StepDirectionIr::Hv,
                        ValueForm::Str(s) if s == "vh" => direction = StepDirectionIr::Vh,
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`direction` expects \"hv\" or \"vh\"",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown StepVertices setting `{name}`"),
                    key_span,
                )),
            }
        }
        StatOptionsIr::StepVertices { direction }
    }

    fn collect_vector_endpoints_options(&mut self, args: &[Arg]) -> StatOptionsIr {
        let mut length_scale = None;
        let mut dup = DupGuard::new(codes::E1404, "VectorEndpoints setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "lengthScale" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() && n >= 0.0 => length_scale = Some(n),
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`lengthScale` expects a non-negative number",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown VectorEndpoints setting `{name}`"),
                    key_span,
                )),
            }
        }
        StatOptionsIr::VectorEndpoints { length_scale }
    }

    fn collect_curve_sample_options(&mut self, args: &[Arg]) -> StatOptionsIr {
        let mut curvature = 0.35;
        let mut points = 16usize;
        let mut dup = DupGuard::new(codes::E1404, "CurveSample setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "curvature" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() => curvature = n,
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`curvature` expects a finite number",
                            node_span(value.syntax()),
                        )),
                    }
                }
                "points" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n)
                            if n.is_finite() && n.fract() == 0.0 && (2.0..=1024.0).contains(&n) =>
                        {
                            points = n as usize
                        }
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`points` expects an integer from 2 to 1024",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown CurveSample setting `{name}`"),
                    key_span,
                )),
            }
        }
        StatOptionsIr::CurveSample { curvature, points }
    }

    fn collect_smooth_options(&mut self, args: &[Arg], stat_span: Span) -> StatOptionsIr {
        let mut method = SmoothMethodIr::Lm;
        let mut span = None;
        let mut span_span = None;
        let mut se = false;
        let mut dup = DupGuard::new(codes::E1404, "Smooth setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "method" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "lm" => method = SmoothMethodIr::Lm,
                        ValueForm::Str(s) if s == "loess" => method = SmoothMethodIr::Loess,
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`method` expects \"lm\" or \"loess\"",
                            node_span(value.syntax()),
                        )),
                    }
                }
                "span" => {
                    let Some(value) = arg.value() else { continue };
                    span_span = Some(node_span(value.syntax()));
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() && n > 0.0 && n <= 1.0 => {
                            span = Some(n)
                        }
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`span` expects a number in (0, 1]",
                            node_span(value.syntax()),
                        )),
                    }
                }
                "se" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Bool(b) => se = b,
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`se` expects a boolean",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown Smooth setting `{name}`"),
                    key_span,
                )),
            }
        }
        // `span` only governs the loess neighborhood; reject it for `lm`.
        if span.is_some() && method != SmoothMethodIr::Loess {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`span` applies only to `method: \"loess\"`",
                span_span.unwrap_or(stat_span),
            ));
        }
        StatOptionsIr::Smooth { method, span, se }
    }

    fn collect_bin2d_options(
        &mut self,
        args: &[Arg],
        stat_span: Span,
        kind: StatKind,
    ) -> StatOptionsIr {
        let (noun, label) = if kind == StatKind::Bin2D {
            ("Bin2D setting", "Bin2D")
        } else {
            ("HexBin setting", "HexBin")
        };
        let mut bins = None;
        let mut dup = DupGuard::new(codes::E1404, noun);
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }
            match name.as_str() {
                "bins" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() && n >= 1.0 => bins = Some(n),
                        _ => self.diag(Diagnostic::error(
                            codes::E1404,
                            "`bins` must be at least 1",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown {label} setting `{name}`"),
                    key_span,
                )),
            }
        }
        let _ = stat_span;
        if kind == StatKind::Bin2D {
            StatOptionsIr::Bin2D { bins }
        } else {
            StatOptionsIr::HexBin { bins }
        }
    }

    fn collect_bin_options(
        &mut self,
        args: &[Arg],
        stat_span: Span,
        input_dtype: Option<DataType>,
    ) -> StatOptionsIr {
        let mut bins = None;
        let mut bin_width = None;
        let mut boundary = None;
        let mut closed = BinClosedIr::Left;
        let mut interval = None;
        let mut dup = DupGuard::new(codes::E1404, "Bin setting");
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());

            if dup.is_duplicate(&mut self.diagnostics, &name, key_span) {
                continue;
            }

            match name.as_str() {
                "bins" | "binWidth" | "boundary" => {
                    let Some(value) = arg.value() else {
                        continue;
                    };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() => {
                            if name == "bins" && n < 1.0 {
                                self.diag(Diagnostic::error(
                                    codes::E1404,
                                    "`bins` must be at least 1",
                                    node_span(value.syntax()),
                                ));
                            } else if name == "binWidth" && n <= 0.0 {
                                self.diag(Diagnostic::error(
                                    codes::E1404,
                                    "`binWidth` must be greater than 0",
                                    node_span(value.syntax()),
                                ));
                            } else {
                                match name.as_str() {
                                    "bins" => bins = Some(n),
                                    "binWidth" => bin_width = Some(n),
                                    _ => boundary = Some(n),
                                }
                            }
                        }
                        form => self.diag(Diagnostic::error(
                            codes::E1404,
                            format!(
                                "`{name}` expects a finite number, found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                "closed" => {
                    let Some(value) = arg.value() else {
                        continue;
                    };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "left" => closed = BinClosedIr::Left,
                        ValueForm::Str(s) if s == "right" => closed = BinClosedIr::Right,
                        ValueForm::Column(column) => {
                            let written = column.name().unwrap_or_else(|| "left".to_string());
                            self.diag(
                                Diagnostic::error(
                                    codes::E1404,
                                    "`closed` expects a quoted string value",
                                    node_span(value.syntax()),
                                )
                                .with_help(format!("write it as a string, e.g. {written:?}")),
                            );
                        }
                        form => self.diag(Diagnostic::error(
                            codes::E1404,
                            format!(
                                "`closed` expects one of [\"left\", \"right\"], found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                "interval" => {
                    let Some(value) = arg.value() else {
                        continue;
                    };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) => match parse_bin_interval(&s) {
                            Some(unit) => interval = Some(unit),
                            None => self.diag(Diagnostic::error(
                                codes::E1404,
                                format!("unknown temporal interval `{s}`"),
                                node_span(value.syntax()),
                            )),
                        },
                        ValueForm::Column(column) => {
                            let written = column.name().unwrap_or_else(|| "month".to_string());
                            self.diag(
                                Diagnostic::error(
                                    codes::E1404,
                                    "`interval` expects a quoted string value",
                                    node_span(value.syntax()),
                                )
                                .with_help(format!("write it as a string, e.g. {written:?}")),
                            );
                        }
                        form => self.diag(Diagnostic::error(
                            codes::E1404,
                            format!(
                                "`interval` expects a temporal interval string, found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1404,
                    format!("unknown Bin setting `{name}`"),
                    key_span,
                )),
            }
        }
        if interval.is_some()
            && !matches!(
                input_dtype,
                Some(DataType::Temporal) | Some(DataType::Unknown) | None
            )
        {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`interval` applies only to temporal `Bin` inputs",
                stat_span,
            ));
        }
        self.check_bin_conflict(
            bins.is_some(),
            bin_width.is_some(),
            boundary.is_some(),
            interval.is_some(),
            stat_span,
        );
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
            interval,
        }
    }

    /// `bins` and `binWidth` are mutually exclusive (spec §15.x). Shared by the
    /// explicit `Bin` derive and `Histogram`/`FreqPoly` lowering.
    pub(super) fn check_bin_conflict(
        &mut self,
        has_bins: bool,
        has_bin_width: bool,
        has_boundary: bool,
        has_interval: bool,
        span: Span,
    ) {
        if has_bins && has_bin_width {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`bins` and `binWidth` must not both be provided",
                span,
            ));
        }
        if has_interval && (has_bins || has_bin_width || has_boundary) {
            self.diag(Diagnostic::error(
                codes::E1404,
                "`interval` must not be combined with `bins`, `binWidth`, or `boundary`",
                span,
            ));
        }
    }
}

/// Output schema for a geometry-producing stat (`Centroid`/`Simplify`): every
/// upstream column passes through (the geometry column carries the computed
/// geometry, scalar columns are unchanged) (spec §15.13).
fn geometry_stat_output_schema(table: &ActiveTable) -> Vec<ColumnDefIr> {
    table
        .names()
        .map(|name| ColumnDefIr {
            name: name.to_string(),
            dtype: table.get(name).unwrap_or(DataType::Unknown),
        })
        .collect()
}

/// Output schema for primitive-construction stats: fixed primitive columns
/// first, followed by non-conflicting source columns so aesthetics such as
/// `stroke: cohort` remain available in the derived table (spec §15.15).
fn primitive_output_schema(
    mut fixed: Vec<ColumnDefIr>,
    table: &ActiveTable,
    reserved: &[&str],
) -> Vec<ColumnDefIr> {
    for name in table.names() {
        if reserved.contains(&name) || fixed.iter().any(|column| column.name == name) {
            continue;
        }
        fixed.push(ColumnDefIr {
            name: name.to_string(),
            dtype: table.get(name).unwrap_or(DataType::Unknown),
        });
    }
    fixed
}

pub(super) fn parse_bin_interval(value: &str) -> Option<BinIntervalIr> {
    match value {
        "minute" => Some(BinIntervalIr::Minute),
        "hour" => Some(BinIntervalIr::Hour),
        "day" => Some(BinIntervalIr::Day),
        "week" => Some(BinIntervalIr::Week),
        "month" => Some(BinIntervalIr::Month),
        "quarter" => Some(BinIntervalIr::Quarter),
        "year" => Some(BinIntervalIr::Year),
        _ => None,
    }
}

pub(super) fn derive_output_names(derive: &DeriveDecl) -> Vec<String> {
    let Some(stat) = derive.stat() else {
        return Vec::new();
    };
    let stat_name = stat.name().unwrap_or_default();
    if stat_name == "StepVertices" {
        let mut names: Vec<String> = stat
            .inputs()
            .into_iter()
            .filter_map(|input| match input {
                AlgebraExpr::Name(name) => name.name(),
                _ => None,
            })
            .take(2)
            .collect();
        names.push("step_group".into());
        return names;
    }
    stat_output_names_for_source(&stat_name)
}

fn derive_input_names(derive: &DeriveDecl) -> Vec<String> {
    derive
        .stat()
        .map(|stat| {
            stat.inputs()
                .into_iter()
                .filter_map(|input| match input {
                    AlgebraExpr::Name(name) => name.name(),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}
