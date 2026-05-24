//! Explicit `Derive` stat analysis (spec §13.4): dependency ordering, stat
//! input arity, setting validation, and derived output schemas.

use std::collections::{HashMap, HashSet};

use algraf_core::{Diagnostic, Span};
use algraf_data::DataType;
use algraf_syntax::ast::{AlgebraExpr, Arg, ChartBlock, ChartItem, DeriveDecl};
use algraf_syntax::node_span;

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer, ValueForm};
use crate::ir::*;

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
                    Diagnostic::error("E1104", format!("duplicate derived table `{name}`"), span)
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
                        "E1501",
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
                        "E1404",
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
            "Bin2D" => StatKind::Bin2D,
            "HexBin" => StatKind::HexBin,
            _ => {
                self.diag(Diagnostic::error(
                    "E1403",
                    format!("unknown stat `{stat_name}`; supported stats are `Bin`, `Smooth`, `Bin2D`, and `HexBin`"),
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
                            "E1404",
                            format!("Bin input column `{}` is not numeric or temporal", col.name),
                            col.span,
                        )),
                    }
                }
                let options = self.collect_bin_options(&stat.args(), stat_span);
                let output_schema = match &input_frame {
                    FrameIr::Vector(column) => bin_output_schema(column.dtype),
                    _ => bin_output_schema(DataType::Float),
                };
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
                                    "E1404",
                                    format!("Smooth input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                (
                    input_frame,
                    self.collect_smooth_options(&stat.args(), stat_span),
                    smooth_output_schema(),
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
                                    "E1404",
                                    format!("{label} input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                let output_schema = if kind == StatKind::Bin2D {
                    bin2d_output_schema()
                } else {
                    hexbin_output_schema()
                };
                (
                    input_frame,
                    self.collect_bin2d_options(&stat.args(), stat_span, kind),
                    output_schema,
                )
            }
            _ => {
                self.diag(Diagnostic::error(
                    "E1403",
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
                "E1404",
                format!("{stat_name} requires exactly one input column"),
                stat_span,
            ));
            return None;
        }
        match &inputs[0] {
            AlgebraExpr::Name(n) => Some(FrameIr::Vector(self.resolve_column(n, table))),
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!("{stat_name} requires a column input"),
                    stat_span,
                ));
                Some(FrameIr::Invalid)
            }
        }
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
                "E1404",
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
                        "E1404",
                        format!("{stat_name} requires column inputs"),
                        stat_span,
                    ));
                    frames.push(FrameIr::Invalid);
                }
            }
        }
        Some(FrameIr::Cartesian(frames))
    }

    fn collect_smooth_options(&mut self, args: &[Arg], stat_span: Span) -> StatOptionsIr {
        let mut dup = DupGuard::new("E1404", "Smooth setting");
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
                        // `lm` is the only supported method (spec §15.x).
                        ValueForm::Str(s) if s == "lm" => {}
                        _ => self.diag(Diagnostic::error(
                            "E1404",
                            "`method` expects \"lm\"",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
                    format!("unknown Smooth setting `{name}`"),
                    key_span,
                )),
            }
        }
        let _ = stat_span;
        StatOptionsIr::Smooth {
            method: SmoothMethodIr::Lm,
        }
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
        let mut dup = DupGuard::new("E1404", noun);
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
                            "E1404",
                            "`bins` must be at least 1",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
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

    fn collect_bin_options(&mut self, args: &[Arg], stat_span: Span) -> StatOptionsIr {
        let mut bins = None;
        let mut bin_width = None;
        let mut boundary = None;
        let mut closed = BinClosedIr::Left;
        let mut dup = DupGuard::new("E1404", "Bin setting");
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
                                    "E1404",
                                    "`bins` must be at least 1",
                                    node_span(value.syntax()),
                                ));
                            } else if name == "binWidth" && n <= 0.0 {
                                self.diag(Diagnostic::error(
                                    "E1404",
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
                            "E1404",
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
                                    "E1404",
                                    "`closed` expects a quoted string value",
                                    node_span(value.syntax()),
                                )
                                .with_help(format!("write it as a string, e.g. {written:?}")),
                            );
                        }
                        form => self.diag(Diagnostic::error(
                            "E1404",
                            format!(
                                "`closed` expects one of [\"left\", \"right\"], found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
                    format!("unknown Bin setting `{name}`"),
                    key_span,
                )),
            }
        }
        self.check_bin_conflict(bins.is_some(), bin_width.is_some(), stat_span);
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
        }
    }

    /// `bins` and `binWidth` are mutually exclusive (spec §15.x). Shared by the
    /// explicit `Bin` derive and `Histogram`/`FreqPoly` lowering.
    pub(super) fn check_bin_conflict(&mut self, has_bins: bool, has_bin_width: bool, span: Span) {
        if has_bins && has_bin_width {
            self.diag(Diagnostic::error(
                "E1404",
                "`bins` and `binWidth` must not both be provided",
                span,
            ));
        }
    }
}

pub(super) fn derive_output_names(derive: &DeriveDecl) -> Vec<String> {
    let Some(stat) = derive.stat() else {
        return Vec::new();
    };
    match stat.name().unwrap_or_default().as_str() {
        "Bin" => bin_output_schema(DataType::Float)
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "Smooth" => smooth_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "Bin2D" => bin2d_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "HexBin" => hexbin_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        _ => Vec::new(),
    }
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

pub(super) fn bin_output_schema(input_dtype: DataType) -> Vec<ColumnDefIr> {
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

pub(super) fn smooth_output_schema() -> Vec<ColumnDefIr> {
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

pub(super) fn bin2d_output_schema() -> Vec<ColumnDefIr> {
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

pub(super) fn hexbin_output_schema() -> Vec<ColumnDefIr> {
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

pub(super) fn bin_boundary_dtype(input_dtype: DataType) -> DataType {
    if input_dtype == DataType::Temporal {
        DataType::Temporal
    } else {
        DataType::Float
    }
}
