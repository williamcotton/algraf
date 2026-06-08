//! The shared analysis context (spec §13).
//!
//! [`Analyzer`] holds the explicit state threaded through every semantic pass:
//! diagnostics, primary and named-table schemas, derived-table schemas, `let`
//! scopes, and the synthetic-name counter. The per-concern passes live in
//! sibling modules and operate on `&mut Analyzer`.

use std::collections::{HashMap, HashSet};

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{ColumnDef, DataType};
use algraf_syntax::ast::{
    AlgebraExpr, AlgebraName, Arg, CallValue, GlyphDecl, LetDecl, LiteralKind, ValueExpr,
};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use crate::ir::*;

/// A resolvable table: column name to type, in declared order.
#[derive(Clone)]
pub(super) struct ActiveTable {
    pub(super) columns: Vec<(String, DataType)>,
    unknown_columns: bool,
}

impl ActiveTable {
    pub(super) fn from_schema(schema: &[ColumnDef]) -> Self {
        ActiveTable {
            columns: schema.iter().map(|c| (c.name.clone(), c.dtype)).collect(),
            unknown_columns: false,
        }
    }

    pub(super) fn from_ir(schema: &[ColumnDefIr]) -> Self {
        ActiveTable {
            columns: schema.iter().map(|c| (c.name.clone(), c.dtype)).collect(),
            unknown_columns: false,
        }
    }

    pub(super) fn empty() -> Self {
        ActiveTable {
            columns: Vec::new(),
            unknown_columns: false,
        }
    }

    pub(super) fn unknown() -> Self {
        ActiveTable {
            columns: Vec::new(),
            unknown_columns: true,
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<DataType> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| *t)
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(n, _)| n.as_str())
    }

    pub(super) fn has_unknown_columns(&self) -> bool {
        self.unknown_columns
    }
}

/// A resolved constant value bound by a `let` declaration (spec §9.6).
#[derive(Clone)]
pub(super) struct LetVar {
    pub(super) value: ConstValue,
}

#[derive(Clone)]
pub(super) struct StyleEntry {
    pub(super) key: String,
    pub(super) arg: Arg,
    pub(super) span: Span,
}

/// The constant value forms a `let` binding may hold (spec §7.10).
#[derive(Clone)]
pub(super) enum ConstValue {
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    NumberArray(Vec<f64>),
    StringArray(Vec<String>),
    Style(Vec<StyleEntry>),
}

impl ConstValue {
    /// Re-express the bound constant as a property [`ValueForm`] for type
    /// checking at the use site (spec §13.9).
    fn to_form(&self) -> ValueForm {
        match self {
            ConstValue::Number(n) => ValueForm::Number(*n),
            ConstValue::Str(s) => ValueForm::Str(s.clone()),
            ConstValue::Bool(b) => ValueForm::Bool(*b),
            ConstValue::Null => ValueForm::Null,
            ConstValue::NumberArray(v) => ValueForm::Array(Some(v.clone())),
            ConstValue::StringArray(v) => ValueForm::StringArray(Some(v.clone())),
            ConstValue::Style(_) => ValueForm::Error,
        }
    }
}

pub(super) enum StyleFragmentLookup {
    Found(Vec<StyleEntry>),
    NotStyle,
    Invalid,
}

pub(super) struct Analyzer<'a> {
    pub(super) primary: &'a [ColumnDef],
    pub(super) allow_unknown_primary_columns: bool,
    /// Schemas of chart-scoped named tables, keyed by declaration name.
    pub(super) table_schemas: &'a HashMap<String, Vec<ColumnDef>>,
    /// Names of declared `Table`s that resolved (used by `space_data`).
    pub(super) table_names: HashSet<String>,
    pub(super) derived: HashMap<String, Vec<ColumnDefIr>>,
    pub(super) reserved_derived_names: HashSet<String>,
    /// Chart-scope `let` bindings, visible in every space (spec §9.6).
    pub(super) chart_vars: HashMap<String, LetVar>,
    /// Space-scope `let` bindings for the space under analysis; these shadow
    /// chart-scope bindings of the same name (spec §9.6).
    pub(super) space_vars: HashMap<String, LetVar>,
    /// Row-context tables for glyph key resolution (spec §14.27). Index 0 is the
    /// current space's active table; later entries are enclosing row contexts.
    pub(super) row_context_tables: Vec<ActiveTable>,
    /// Chart-scoped `Glyph` declarations, keyed by name (spec §7.11, §13.8).
    pub(super) glyphs: HashMap<String, GlyphDecl>,
    /// Stack of glyph names currently being expanded, used to detect recursive
    /// glyph marks (spec §14.27, `E2210`).
    pub(super) glyph_stack: Vec<String>,
    pub(super) synthetic_counter: usize,
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl<'a> Analyzer<'a> {
    pub(super) fn new(
        primary: &'a [ColumnDef],
        table_schemas: &'a HashMap<String, Vec<ColumnDef>>,
        allow_unknown_primary_columns: bool,
    ) -> Self {
        Analyzer {
            primary,
            allow_unknown_primary_columns,
            table_schemas,
            table_names: HashSet::new(),
            derived: HashMap::new(),
            reserved_derived_names: HashSet::new(),
            chart_vars: HashMap::new(),
            space_vars: HashMap::new(),
            row_context_tables: Vec::new(),
            glyphs: HashMap::new(),
            glyph_stack: Vec::new(),
            synthetic_counter: 0,
            diagnostics: Vec::new(),
        }
    }

    pub(super) fn primary_table(&self) -> ActiveTable {
        if self.allow_unknown_primary_columns {
            ActiveTable::unknown()
        } else {
            ActiveTable::from_schema(self.primary)
        }
    }

    pub(super) fn diag(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    /// Allocate a unique synthetic derived-table name with the given prefix,
    /// skipping any name already taken by a user-authored or earlier synthetic
    /// table (spec §15.x).
    pub(super) fn next_synthetic(&mut self, prefix: &str) -> String {
        loop {
            let name = format!("__{}_{}", prefix, self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    // --- Let bindings (spec §7.10, §9.6) ---

    /// Evaluate a list of `let` declarations in one scope into a name→value map,
    /// reporting duplicate bindings (E1702) and non-constant values (E1701).
    pub(super) fn collect_let_decls(&mut self, decls: &[LetDecl]) -> HashMap<String, LetVar> {
        let mut vars: HashMap<String, LetVar> = HashMap::new();
        let mut spans: HashMap<String, Span> = HashMap::new();
        for decl in decls {
            let Some(name) = decl.name() else { continue };
            let name_span = decl.name_span().unwrap_or_else(|| node_span(decl.syntax()));
            if let Some(&first) = spans.get(&name) {
                self.diag(
                    Diagnostic::error(
                        codes::E1702,
                        format!("duplicate `let` binding `{name}`"),
                        name_span,
                    )
                    .with_related(first, "first bound here"),
                );
                continue;
            }
            spans.insert(name.clone(), name_span);
            if let Some(value) = self.eval_let_value(decl) {
                vars.insert(name, LetVar { value });
            }
        }
        vars
    }

    /// Resolve a `let` binding's value to a constant, or emit E1701. Variables
    /// hold constant values only in this version (spec §7.10): column mappings,
    /// algebra, and references to other variables are rejected.
    fn eval_let_value(&mut self, decl: &LetDecl) -> Option<ConstValue> {
        let value = decl.value()?;
        let span = node_span(value.syntax());
        if let ValueExpr::Call(call) = &value {
            if call.name().as_deref() == Some("Style") {
                return self.eval_style_call(call).map(ConstValue::Style);
            }
        }
        match ValueForm::of(&value) {
            ValueForm::Number(n) => Some(ConstValue::Number(n)),
            ValueForm::Str(s) => Some(ConstValue::Str(s)),
            ValueForm::Bool(b) => Some(ConstValue::Bool(b)),
            ValueForm::Null => Some(ConstValue::Null),
            ValueForm::Array(Some(v)) => Some(ConstValue::NumberArray(v)),
            ValueForm::StringArray(Some(v)) => Some(ConstValue::StringArray(v)),
            _ => {
                self.diag(
                    Diagnostic::error(
                        codes::E1701,
                        "`let` binding value must be a constant literal or array",
                        span,
                    )
                    .with_help(
                        "variables hold constants such as \"#3366cc\", 0.4, true, or [1, 2]",
                    ),
                );
                None
            }
        }
    }

    /// Substitute an in-scope `let` binding for a bare-identifier property
    /// value. Space-scope bindings shadow chart-scope ones; quoted identifiers
    /// are always column references and are never substituted (spec §9.6).
    pub(super) fn substitute_var(&self, form: ValueForm) -> ValueForm {
        if let ValueForm::Column(name) = &form {
            if !name.is_quoted() {
                if let Some(var_name) = name.name() {
                    if let Some(var) = self
                        .space_vars
                        .get(&var_name)
                        .or_else(|| self.chart_vars.get(&var_name))
                    {
                        return var.value.to_form();
                    }
                }
            }
        }
        form
    }

    pub(super) fn style_fragment_for_value(&mut self, value: &ValueExpr) -> StyleFragmentLookup {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(name)) if !name.is_quoted() => {
                let Some(var_name) = name.name() else {
                    return StyleFragmentLookup::NotStyle;
                };
                match self
                    .space_vars
                    .get(&var_name)
                    .or_else(|| self.chart_vars.get(&var_name))
                {
                    Some(LetVar {
                        value: ConstValue::Style(entries),
                    }) => StyleFragmentLookup::Found(entries.clone()),
                    _ => StyleFragmentLookup::NotStyle,
                }
            }
            ValueExpr::Call(call) if call.name().as_deref() == Some("Style") => self
                .eval_style_call(call)
                .map(StyleFragmentLookup::Found)
                .unwrap_or(StyleFragmentLookup::Invalid),
            _ => StyleFragmentLookup::NotStyle,
        }
    }

    pub(super) fn eval_style_call(&mut self, call: &CallValue) -> Option<Vec<StyleEntry>> {
        let mut entries = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut ok = true;
        for arg in call.args() {
            let arg_span = node_span(arg.syntax());
            let Some(key) = arg.key() else {
                self.diag(Diagnostic::error(
                    codes::E1706,
                    "`Style(...)` entries must be named",
                    arg_span,
                ));
                ok = false;
                continue;
            };
            if key == "style" {
                self.diag(Diagnostic::error(
                    codes::E1706,
                    "`Style(...)` cannot contain `style:`",
                    arg_span,
                ));
                ok = false;
                continue;
            }
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        codes::E1706,
                        format!("duplicate `Style` property `{key}`"),
                        arg_span,
                    )
                    .with_related(first, "first defined here"),
                );
                ok = false;
                continue;
            }
            if matches!(arg.value(), Some(ValueExpr::Call(_))) {
                self.diag(Diagnostic::error(
                    codes::E1706,
                    "`Style(...)` values must be literals or column mappings",
                    arg_span,
                ));
                ok = false;
                continue;
            }
            seen.insert(key.clone(), arg_span);
            entries.push(StyleEntry {
                key,
                arg,
                span: arg_span,
            });
        }
        ok.then_some(entries)
    }
}

/// A classified property value form.
pub(super) enum ValueForm {
    Column(AlgebraName),
    ComplexAlgebra,
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    Array(Option<Vec<f64>>),
    StringArray(Option<Vec<String>>),
    Stdin,
    /// A nested call value such as `Text(size: 12)` (spec §20.8); only valid in
    /// theme override positions, handled directly there.
    Call,
    Error,
}

impl ValueForm {
    pub(super) fn of(value: &ValueExpr) -> ValueForm {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(n)) => ValueForm::Column(n.clone()),
            ValueExpr::Algebra(AlgebraExpr::Error(_)) => ValueForm::Error,
            ValueExpr::Algebra(_) => ValueForm::ComplexAlgebra,
            ValueExpr::Literal(lit) => match lit.kind() {
                Some(LiteralKind::Number) => ValueForm::Number(
                    lit.text()
                        .and_then(|t| t.parse::<f64>().ok())
                        .unwrap_or(0.0),
                ),
                Some(LiteralKind::String) => {
                    ValueForm::Str(string_value(&lit.text().unwrap_or_default()))
                }
                Some(LiteralKind::Bool) => ValueForm::Bool(lit.text().as_deref() == Some("true")),
                Some(LiteralKind::Null) | None => ValueForm::Null,
            },
            ValueExpr::Stdin(_) => ValueForm::Stdin,
            ValueExpr::Array(array) => {
                let mut nums = Vec::new();
                let mut strings = Vec::new();
                let mut all_numeric = true;
                let mut all_strings = true;
                for item in array.values() {
                    match ValueForm::of(&item) {
                        ValueForm::Number(n) => nums.push(n),
                        _ => all_numeric = false,
                    }
                    match ValueForm::of(&item) {
                        ValueForm::Str(s) => strings.push(s),
                        _ => all_strings = false,
                    }
                }
                if all_strings {
                    ValueForm::StringArray(Some(strings))
                } else {
                    ValueForm::Array(all_numeric.then_some(nums))
                }
            }
            // Map literals are valid only in `Scale(range:/labels:)` positions,
            // handled directly there; elsewhere they are an invalid value.
            ValueExpr::Map(_) => ValueForm::Error,
            ValueExpr::Call(_) => ValueForm::Call,
            ValueExpr::Error(_) => ValueForm::Error,
        }
    }

    pub(super) fn describe(&self) -> &'static str {
        match self {
            ValueForm::Column(_) => "a column mapping",
            ValueForm::ComplexAlgebra => "an algebra expression",
            ValueForm::Number(_) => "a number",
            ValueForm::Str(_) => "a string",
            ValueForm::Bool(_) => "a boolean",
            ValueForm::Null => "null",
            ValueForm::Array(_) | ValueForm::StringArray(_) => "an array",
            ValueForm::Stdin => "the stdin sentinel",
            ValueForm::Call => "a nested call",
            ValueForm::Error => "an invalid value",
        }
    }
}
