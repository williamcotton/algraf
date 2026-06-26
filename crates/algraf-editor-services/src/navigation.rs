// --- Navigation: definition, references, highlight, rename (spec §21.8) -----

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use algraf_core::Span;
use algraf_syntax::ast::{
    AlgebraName, Arg, DeriveDecl, LetDecl, LiteralKind, Root, ValueExpr, VariableRef,
};
use algraf_syntax::{node_span, parse, SyntaxKind, SyntaxNode};
use lsp_types::{GotoDefinitionResponse, Location, Range, TextEdit, Url, WorkspaceEdit};

use crate::document::DocumentState;
use crate::positions::{offset_to_position, span_to_range};

/// A `Derive` declaration site within the document.
struct DeriveSite {
    name: String,
    /// Span of the table-name identifier token (the navigation target).
    name_span: Span,
}

/// A name occurrence carrying its byte span.
struct NameRef {
    name: String,
    span: Span,
}

/// A `let` declaration site, tagged with its lexical scope (spec §9.6).
pub struct LetSite {
    pub name: String,
    /// Span of the variable-name identifier token (the navigation target).
    name_span: Span,
    value_kind: String,
    /// The lexical scope that owns this binding.
    scope: LetScope,
}

/// A visible `let` binding for completion and hover surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LetBindingInfo {
    pub name: String,
    pub name_span: Span,
    pub scope_label: &'static str,
    pub value_kind: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LetScope {
    Document,
    Chart(usize),
    Space(usize),
}

#[derive(Clone, Copy)]
struct RefScope {
    chart: Option<usize>,
    space: Option<usize>,
}

/// A variable reference in a property value position, tagged with the scope it
/// appears in so it can be resolved against the right `let` binding.
struct VarRefSite {
    name: String,
    span: Span,
    scope: RefScope,
}

/// An index of all in-document name occurrences, partitioned by namespace
/// (spec §9.4). Built by walking the CST so spans are byte-accurate.
#[derive(Default)]
pub struct NameIndex {
    /// `Derive` declarations (derived-table definitions).
    derives: Vec<DeriveSite>,
    /// `let` declarations (variable definitions).
    pub lets: Vec<LetSite>,
    /// `data:` references to a derived table (e.g. `Space(..., data: binned)`).
    table_refs: Vec<NameRef>,
    /// Column references in frames, aesthetic mappings, and stat inputs.
    column_refs: Vec<NameRef>,
    /// Variable references in property value positions.
    var_refs: Vec<VarRefSite>,
}

pub fn build_name_index(root: &SyntaxNode) -> NameIndex {
    let mut index = NameIndex::default();

    // First pass: collect `Derive` and `let` declarations so variable
    // references can be resolved against in-scope bindings in the second pass.
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::DERIVE_DECL => {
                if let Some(decl) = DeriveDecl::cast(node.clone()) {
                    if let (Some(name), Some(span)) = (decl.name(), decl.name_span()) {
                        index.derives.push(DeriveSite {
                            name,
                            name_span: span,
                        });
                    }
                }
            }
            SyntaxKind::LET_DECL => {
                if let Some(decl) = LetDecl::cast(node.clone()) {
                    if let (Some(name), Some(span)) = (decl.name(), decl.name_span()) {
                        index.lets.push(LetSite {
                            name,
                            name_span: span,
                            value_kind: let_value_kind(&decl),
                            scope: binding_scope(&node),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let derived_names: HashSet<String> = index
        .derives
        .iter()
        .map(|derive| derive.name.clone())
        .collect();
    for node in root.descendants() {
        if node.kind() != SyntaxKind::DERIVE_DECL {
            continue;
        }
        let Some(decl) = DeriveDecl::cast(node.clone()) else {
            continue;
        };
        let (Some(name), Some(span)) = (decl.source_name(), decl.source_name_span()) else {
            continue;
        };
        if derived_names.contains(&name) {
            index.table_refs.push(NameRef { name, span });
        }
    }

    // Second pass: classify identifier occurrences. Bare identifiers are data
    // table or column references; only `$name` nodes are `let` references
    // (spec §9.6).
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::ALGEBRA_NAME => {
                let Some(algebra) = AlgebraName::cast(node.clone()) else {
                    continue;
                };
                let (Some(name), Some(span)) = (algebra.name(), algebra.ident_span()) else {
                    continue;
                };
                if is_data_arg_value(&node) {
                    index.table_refs.push(NameRef { name, span });
                } else {
                    index.column_refs.push(NameRef { name, span });
                }
            }
            SyntaxKind::VARIABLE_REF => {
                let Some(var) = VariableRef::cast(node.clone()) else {
                    continue;
                };
                let Some(name) = var.name() else { continue };
                index.var_refs.push(VarRefSite {
                    name,
                    span: var.reference_span(),
                    scope: reference_scope(&node),
                });
            }
            _ => {}
        }
    }
    index
}

fn let_value_kind(decl: &LetDecl) -> String {
    match decl.value() {
        Some(ValueExpr::Literal(lit)) => match lit.kind() {
            Some(LiteralKind::Number) => "number",
            Some(LiteralKind::String) => "string",
            Some(LiteralKind::Bool) => "boolean",
            Some(LiteralKind::Null) => "null",
            None => "literal",
        },
        Some(ValueExpr::Array(_)) => "array",
        Some(ValueExpr::Call(call)) => match call.name().as_deref() {
            Some("Style") => "style fragment",
            Some("Theme") => "theme",
            Some(_) => "call",
            None => "call",
        },
        Some(ValueExpr::Algebra(_)) => "column or algebra",
        Some(ValueExpr::Stdin(_)) => "input",
        Some(ValueExpr::Variable(_)) => "let reference",
        Some(ValueExpr::Map(_)) => "map",
        Some(ValueExpr::Error(_)) | None => "value",
    }
    .to_string()
}

fn binding_scope(node: &SyntaxNode) -> LetScope {
    let scope = reference_scope(node);
    if let Some(space) = scope.space {
        LetScope::Space(space)
    } else if let Some(chart) = scope.chart {
        LetScope::Chart(chart)
    } else {
        LetScope::Document
    }
}

/// The start offsets of the nearest enclosing chart and space blocks.
fn reference_scope(node: &SyntaxNode) -> RefScope {
    let mut current = node.parent();
    let mut chart = None;
    let mut space = None;
    while let Some(parent) = current {
        match parent.kind() {
            SyntaxKind::SPACE_BLOCK if space.is_none() => {
                space = Some(u32::from(parent.text_range().start()) as usize);
            }
            SyntaxKind::CHART_BLOCK if chart.is_none() => {
                chart = Some(u32::from(parent.text_range().start()) as usize);
            }
            _ => {}
        }
        current = parent.parent();
    }
    RefScope { chart, space }
}

/// Resolve which `let` binding a reference named `name` in scope `ref_scope`
/// binds to: space scope shadows chart scope, which shadows document scope
/// (spec §9.6). Returns the binding's scope, or `None` if undefined.
fn resolve_binding_scope(lets: &[LetSite], name: &str, ref_scope: RefScope) -> Option<LetScope> {
    if let Some(space) = ref_scope.space {
        if lets
            .iter()
            .any(|site| site.name == name && site.scope == LetScope::Space(space))
        {
            return Some(LetScope::Space(space));
        }
    }
    if let Some(chart) = ref_scope.chart {
        if lets
            .iter()
            .any(|site| site.name == name && site.scope == LetScope::Chart(chart))
        {
            return Some(LetScope::Chart(chart));
        }
    }
    if lets.iter().any(|site| {
        site.name == name
            && matches!(
                site.scope,
                // Sources that predate document scope treated non-space lets
                // as chart-scope. Keep navigation compatible by resolving a
                // reference outside a chart to document bindings only.
                LetScope::Document
            )
    }) {
        return Some(LetScope::Document);
    }
    None
}

fn reference_scope_at(root: &SyntaxNode, offset: usize) -> RefScope {
    let mut chart = None;
    let mut space = None;
    for node in root.descendants() {
        let span = node_span(&node);
        if !span.contains(offset) && span.end != offset {
            continue;
        }
        match node.kind() {
            SyntaxKind::SPACE_BLOCK => {
                space = Some(u32::from(node.text_range().start()) as usize);
            }
            SyntaxKind::CHART_BLOCK => {
                chart = Some(u32::from(node.text_range().start()) as usize);
            }
            _ => {}
        }
    }
    RefScope { chart, space }
}

fn let_scope_label(scope: LetScope) -> &'static str {
    match scope {
        LetScope::Document => "document",
        LetScope::Chart(_) => "chart",
        LetScope::Space(_) => "space",
    }
}

fn binding_info(site: &LetSite) -> LetBindingInfo {
    LetBindingInfo {
        name: site.name.clone(),
        name_span: site.name_span,
        scope_label: let_scope_label(site.scope),
        value_kind: site.value_kind.clone(),
    }
}

fn visible_let_sites(index: &NameIndex, scope: RefScope) -> Vec<&LetSite> {
    let mut by_name: HashMap<String, &LetSite> = HashMap::new();
    for site in &index.lets {
        if site.scope == LetScope::Document {
            by_name.insert(site.name.clone(), site);
        }
    }
    if let Some(chart) = scope.chart {
        for site in &index.lets {
            if site.scope == LetScope::Chart(chart) {
                by_name.insert(site.name.clone(), site);
            }
        }
    }
    if let Some(space) = scope.space {
        for site in &index.lets {
            if site.scope == LetScope::Space(space) {
                by_name.insert(site.name.clone(), site);
            }
        }
    }
    let mut sites = by_name.into_values().collect::<Vec<_>>();
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    sites
}

pub fn visible_let_bindings(text: &str, offset: usize) -> Vec<LetBindingInfo> {
    let root = parse(text).syntax();
    let index = build_name_index(&root);
    visible_let_sites(&index, reference_scope_at(&root, offset))
        .into_iter()
        .map(binding_info)
        .collect()
}

pub fn let_binding_at_reference(text: &str, offset: usize) -> Option<LetBindingInfo> {
    let (_, info) = let_binding_reference_at(text, offset)?;
    Some(info)
}

pub fn let_binding_reference_at(text: &str, offset: usize) -> Option<(Span, LetBindingInfo)> {
    let root = parse(text).syntax();
    let index = build_name_index(&root);
    for reference in &index.var_refs {
        if !reference.span.contains(offset) && reference.span.end != offset {
            continue;
        }
        let scope = resolve_binding_scope(&index.lets, &reference.name, reference.scope)?;
        let site = index
            .lets
            .iter()
            .find(|site| site.name == reference.name && site.scope == scope)?;
        return Some((reference.span, binding_info(site)));
    }
    None
}

/// Whether an `ALGEBRA_NAME` node sits in the value position of a `data:`
/// argument (a derived-table reference) rather than a column position.
fn is_data_arg_value(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::ARG {
            return Arg::cast(parent).and_then(|arg| arg.key()).as_deref() == Some("data");
        }
        current = parent.parent();
    }
    false
}

/// What the identifier under the cursor refers to.
enum Target {
    DerivedTable(String),
    /// A `let` variable, identified by name and the binding's scope.
    Variable {
        name: String,
        scope: LetScope,
    },
    Column(String),
    /// The chart's `data:` string literal.
    DataPath,
}

fn target_at(index: &NameIndex, root: &SyntaxNode, offset: usize) -> Option<Target> {
    for derive in &index.derives {
        if derive.name_span.contains(offset) {
            return Some(Target::DerivedTable(derive.name.clone()));
        }
    }
    for site in &index.lets {
        if site.name_span.contains(offset) {
            return Some(Target::Variable {
                name: site.name.clone(),
                scope: site.scope,
            });
        }
    }
    for reference in &index.var_refs {
        if reference.span.contains(offset) {
            let scope = resolve_binding_scope(&index.lets, &reference.name, reference.scope)?;
            return Some(Target::Variable {
                name: reference.name.clone(),
                scope,
            });
        }
    }
    for reference in &index.table_refs {
        if reference.span.contains(offset) {
            return Some(Target::DerivedTable(reference.name.clone()));
        }
    }
    for reference in &index.column_refs {
        if reference.span.contains(offset) {
            return Some(Target::Column(reference.name.clone()));
        }
    }
    if chart_data_literal_span(root).is_some_and(|span| span.contains(offset)) {
        return Some(Target::DataPath);
    }
    None
}

/// The span of the chart-level `data:` string literal, if present.
fn chart_data_literal_span(root: &SyntaxNode) -> Option<Span> {
    let chart = Root::cast(root.clone())?.chart()?;
    for arg in chart.args() {
        if arg.key().as_deref() == Some("data") {
            if let Some(ValueExpr::Literal(literal)) = arg.value() {
                if literal.kind() == Some(LiteralKind::String) {
                    return literal.token_span();
                }
            }
        }
    }
    None
}

pub fn definition_at(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
) -> Option<GotoDefinitionResponse> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    match target_at(&index, &root, offset)? {
        Target::DataPath => {
            let path = state.data_path.as_ref()?;
            let target_uri = state
                .virtual_file_for_path(path)
                .map(|file| file.uri.clone())
                .or_else(|| file_url_from_path(path))?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: Range::default(),
            }))
        }
        Target::Variable { name, scope } => {
            let site = index
                .lets
                .iter()
                .find(|site| site.name == name && site.scope == scope)?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_range(&state.text, site.name_span),
            }))
        }
        Target::DerivedTable(name) => {
            let site = index.derives.iter().find(|derive| derive.name == name)?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_range(&state.text, site.name_span),
            }))
        }
        Target::Column(name) => {
            let producers = derives_producing(state, &name);
            match producers.len() {
                // A derived column jumps to the `Derive` that produces it.
                1 => {
                    let site = index
                        .derives
                        .iter()
                        .find(|derive| derive.name == producers[0])?;
                    Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: span_to_range(&state.text, site.name_span),
                    }))
                }
                // Ambiguous: refuse rather than guess (spec §21.8).
                n if n > 1 => None,
                // A source column opens the CSV header (best effort).
                _ => {
                    let (path, range) = csv_header_location(state, &name)?;
                    let target_uri = state
                        .virtual_file_for_path(&path)
                        .map(|file| file.uri.clone())
                        .or_else(|| file_url_from_path(&path))?;
                    Some(GotoDefinitionResponse::Scalar(Location {
                        uri: target_uri,
                        range,
                    }))
                }
            }
        }
    }
}

/// Names of in-document `Derive` tables whose output schema contains `column`.
fn derives_producing(state: &DocumentState, column: &str) -> Vec<String> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .map(|ir| {
            ir.derived_tables
                .iter()
                .filter(|table| table.output_schema.iter().any(|col| col.name == column))
                .map(|table| table.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Locate a column's header within the resolved CSV file (best effort).
fn csv_header_location(state: &DocumentState, name: &str) -> Option<(PathBuf, Range)> {
    let path = state.data_path.clone()?;
    let content = state
        .virtual_file_for_path(&path)
        .map(|file| file.text.clone())
        .or_else(|| std::fs::read_to_string(&path).ok())?;
    let header = content.lines().next()?;
    let (start, end) = csv_header_field(header, name)?;
    Some((
        path,
        Range {
            start: offset_to_position(&content, start),
            end: offset_to_position(&content, end),
        },
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn file_url_from_path(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

#[cfg(target_arch = "wasm32")]
fn file_url_from_path(_path: &Path) -> Option<Url> {
    None
}

/// Byte range of the header field equal to `name` in a CSV header line,
/// honoring minimal RFC-4180 double-quoting.
fn csv_header_field(header: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = header.as_bytes();
    let mut field_start = 0usize;
    let mut value = String::new();
    let mut in_quotes = false;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        match ch {
            '"' => {
                if in_quotes && bytes.get(idx + 1) == Some(&b'"') {
                    value.push('"');
                    idx += 1;
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                if value == name {
                    return Some((field_start, idx));
                }
                value.clear();
                field_start = idx + 1;
            }
            other => value.push(other),
        }
        idx += 1;
    }
    (value == name).then_some((field_start, header.len()))
}

/// A reference site for highlight/references, flagged if it is a declaration.
pub struct RefSite {
    pub span: Span,
    pub is_decl: bool,
}

pub fn reference_sites(state: &DocumentState, offset: usize) -> Option<Vec<RefSite>> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    match target_at(&index, &root, offset)? {
        Target::DataPath => None,
        Target::Variable { name, scope } => {
            let mut sites = Vec::new();
            for site in &index.lets {
                if site.name == name && site.scope == scope {
                    sites.push(RefSite {
                        span: site.name_span,
                        is_decl: true,
                    });
                }
            }
            for reference in &index.var_refs {
                if reference.name == name
                    && resolve_binding_scope(&index.lets, &reference.name, reference.scope)
                        == Some(scope)
                {
                    sites.push(RefSite {
                        span: reference.span,
                        is_decl: false,
                    });
                }
            }
            Some(sites)
        }
        Target::DerivedTable(name) => {
            let mut sites = Vec::new();
            for derive in &index.derives {
                if derive.name == name {
                    sites.push(RefSite {
                        span: derive.name_span,
                        is_decl: true,
                    });
                }
            }
            for reference in &index.table_refs {
                if reference.name == name {
                    sites.push(RefSite {
                        span: reference.span,
                        is_decl: false,
                    });
                }
            }
            Some(sites)
        }
        Target::Column(name) => Some(
            index
                .column_refs
                .iter()
                .filter(|reference| reference.name == name)
                .map(|reference| RefSite {
                    span: reference.span,
                    is_decl: false,
                })
                .collect(),
        ),
    }
}

/// The span of a renameable identifier under the cursor, if one exists. Only
/// derived-table names are user-introduced and therefore renameable.
pub fn renameable_at(state: &DocumentState, offset: usize) -> Option<Span> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    for derive in &index.derives {
        if derive.name_span.contains(offset) {
            return Some(derive.name_span);
        }
    }
    for reference in &index.table_refs {
        if reference.span.contains(offset) {
            return Some(reference.span);
        }
    }
    for site in &index.lets {
        if site.name_span.contains(offset) {
            return Some(site.name_span);
        }
    }
    for reference in &index.var_refs {
        if reference.span.contains(offset) {
            return Some(reference.span);
        }
    }
    None
}

pub fn rename_edits(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let sites = reference_sites(state, offset)?;
    if sites.is_empty() {
        return None;
    }
    let edits: Vec<TextEdit> = sites
        .into_iter()
        .map(|site| {
            let source_text = &state.text[site.span.start..site.span.end];
            let new_text = if !site.is_decl && source_text.trim_start().starts_with('$') {
                format!("${new_name}")
            } else {
                new_name.to_string()
            };
            TextEdit {
                range: span_to_range(&state.text, site.span),
                new_text,
            }
        })
        .collect();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentState;

    fn state(text: &str) -> DocumentState {
        DocumentState {
            text: text.to_string(),
            version: 0,
            parse: None,
            analysis: None,
            primary_schema: None,
            table_schemas: Default::default(),
            source_previews: Default::default(),
            data_path: None,
            virtual_files: Default::default(),
            has_external_schema_sources: false,
            diagnostics: Vec::new(),
        }
    }

    fn uri() -> Url {
        Url::parse("file:///doc.ag").unwrap()
    }

    #[test]
    fn build_name_index_records_let_declaration() {
        let text =
            "Chart(data: \"p.csv\") {\n  let c = \"#111\"\n  Space(x * y) { Point(fill: $c) }\n}";
        let index = build_name_index(&parse(text).syntax());
        assert!(index.lets.iter().any(|site| site.name == "c"));
        assert!(!index.var_refs.is_empty());
    }

    #[test]
    fn rename_let_rewrites_declaration_and_use() {
        let text =
            "Chart(data: \"p.csv\") {\n  let c = \"#111\"\n  Space(x * y) { Point(fill: $c) }\n}";
        let state = state(text);
        let offset = text.find("let c").unwrap() + 4; // on the `c` of the decl
        assert!(renameable_at(&state, offset).is_some());
        let edit = rename_edits(&state, &uri(), offset, "color").expect("rename");
        let edits = &edit.changes.unwrap()[&uri()];
        // Declaration plus the one `fill: $c` use are both rewritten.
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|e| e.new_text == "color"));
        assert!(edits.iter().any(|e| e.new_text == "$color"));
    }

    #[test]
    fn derived_from_reference_participates_in_rename() {
        let text =
            "Chart(data: \"p.csv\") {\n  Derive bins = Bin(value)\n  Derive trend from bins = Smooth(bin_center, count)\n}";
        let state = state(text);
        let offset = text.find("from bins").unwrap() + "from ".len();
        assert!(renameable_at(&state, offset).is_some());
        let edit = rename_edits(&state, &uri(), offset, "binned").expect("rename");
        let edits = &edit.changes.unwrap()[&uri()];
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "binned"));
    }
}
