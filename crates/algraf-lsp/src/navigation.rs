// --- Navigation: definition, references, highlight, rename (spec §21.8) -----

use std::collections::HashMap;
use std::path::PathBuf;

use algraf_core::Span;
use algraf_syntax::ast::{AlgebraName, Arg, DeriveDecl, LetDecl, LiteralKind, Root, ValueExpr};
use algraf_syntax::{parse, SyntaxKind, SyntaxNode};
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Range, TextEdit, Url, WorkspaceEdit};

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
pub(crate) struct LetSite {
    pub(crate) name: String,
    /// Span of the variable-name identifier token (the navigation target).
    name_span: Span,
    /// The start offset of the enclosing `Space` block, or `None` for a
    /// chart-scope binding.
    scope: Option<usize>,
}

/// A variable reference in a property value position, tagged with the scope it
/// appears in so it can be resolved against the right `let` binding.
struct VarRefSite {
    name: String,
    span: Span,
    scope: Option<usize>,
}

/// An index of all in-document name occurrences, partitioned by namespace
/// (spec §9.4). Built by walking the CST so spans are byte-accurate.
#[derive(Default)]
pub(crate) struct NameIndex {
    /// `Derive` declarations (derived-table definitions).
    derives: Vec<DeriveSite>,
    /// `let` declarations (variable definitions).
    pub(crate) lets: Vec<LetSite>,
    /// `data:` references to a derived table (e.g. `Space(..., data: binned)`).
    table_refs: Vec<NameRef>,
    /// Column references in frames, aesthetic mappings, and stat inputs.
    column_refs: Vec<NameRef>,
    /// Variable references in property value positions.
    var_refs: Vec<VarRefSite>,
}

pub(crate) fn build_name_index(root: &SyntaxNode) -> NameIndex {
    let mut index = NameIndex::default();

    // First pass: collect `Derive` and `let` declarations so variable
    // references can be resolved against in-scope bindings in the second pass.
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::DERIVE_DECL => {
                if let Some(decl) = DeriveDecl::cast(node.clone()) {
                    if let (Some(name), Some(span)) = (decl.name(), derive_name_span(&node)) {
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
                            scope: enclosing_space_start(&node),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Second pass: classify identifier occurrences. A bare identifier in a
    // property value position that names an in-scope `let` is a variable
    // reference; otherwise it is a column reference (spec §9.6).
    for node in root.descendants() {
        if node.kind() != SyntaxKind::ALGEBRA_NAME {
            continue;
        }
        let Some(algebra) = AlgebraName::cast(node.clone()) else {
            continue;
        };
        let (Some(name), Some(span)) = (algebra.name(), algebra.ident_span()) else {
            continue;
        };
        if is_data_arg_value(&node) {
            index.table_refs.push(NameRef { name, span });
            continue;
        }
        let scope = enclosing_space_start(&node);
        if !algebra.is_quoted()
            && is_property_value(&node)
            && resolve_binding_scope(&index.lets, &name, scope).is_some()
        {
            index.var_refs.push(VarRefSite { name, span, scope });
        } else {
            index.column_refs.push(NameRef { name, span });
        }
    }
    index
}

/// The start offset of the nearest enclosing `Space` block, or `None` when the
/// node sits directly in chart scope.
fn enclosing_space_start(node: &SyntaxNode) -> Option<usize> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::SPACE_BLOCK {
            return Some(u32::from(parent.text_range().start()) as usize);
        }
        current = parent.parent();
    }
    None
}

/// Whether an `ALGEBRA_NAME` sits in a property value position (the value of an
/// argument other than `data:`), as opposed to a `Space` frame or stat input.
fn is_property_value(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            SyntaxKind::ARG => {
                return Arg::cast(parent).and_then(|arg| arg.key()).as_deref() != Some("data");
            }
            SyntaxKind::SPACE_BLOCK | SyntaxKind::STAT_CALL => return false,
            _ => {}
        }
        current = parent.parent();
    }
    false
}

/// Resolve which `let` binding a reference named `name` in scope `ref_scope`
/// binds to: a space-scope binding in the same space shadows a chart-scope one
/// (spec §9.6). Returns the binding's scope, or `None` if undefined.
fn resolve_binding_scope(
    lets: &[LetSite],
    name: &str,
    ref_scope: Option<usize>,
) -> Option<Option<usize>> {
    if let Some(space) = ref_scope {
        if lets
            .iter()
            .any(|site| site.name == name && site.scope == Some(space))
        {
            return Some(Some(space));
        }
    }
    if lets
        .iter()
        .any(|site| site.name == name && site.scope.is_none())
    {
        return Some(None);
    }
    None
}

/// The span of the table-name identifier inside a `DERIVE_DECL` node. The
/// `Derive` keyword is its own token kind, so the first `IDENT` is the name.
fn derive_name_span(node: &SyntaxNode) -> Option<Span> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| {
            let range = token.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
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
        scope: Option<usize>,
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
            let scope = resolve_binding_scope(&index.lets, &reference.name, reference.scope)
                .unwrap_or(reference.scope);
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

pub(crate) fn definition_at(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
) -> Option<GotoDefinitionResponse> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    match target_at(&index, &root, offset)? {
        Target::DataPath => {
            let path = state.data_path.as_ref()?;
            let target_uri = Url::from_file_path(path).ok()?;
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
                    let target_uri = Url::from_file_path(path).ok()?;
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
    let content = std::fs::read_to_string(&path).ok()?;
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
pub(crate) struct RefSite {
    pub(crate) span: Span,
    pub(crate) is_decl: bool,
}

pub(crate) fn reference_sites(state: &DocumentState, offset: usize) -> Option<Vec<RefSite>> {
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
pub(crate) fn renameable_at(state: &DocumentState, offset: usize) -> Option<Span> {
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

pub(crate) fn rename_edits(
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
        .map(|site| TextEdit {
            range: span_to_range(&state.text, site.span),
            new_text: new_name.to_string(),
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
            data_path: None,
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
            "Chart(data: \"p.csv\") {\n  let c = \"#111\"\n  Space(x * y) { Point(fill: c) }\n}";
        let index = build_name_index(&parse(text).syntax());
        assert!(index.lets.iter().any(|site| site.name == "c"));
        assert!(!index.var_refs.is_empty());
    }

    #[test]
    fn rename_let_rewrites_declaration_and_use() {
        let text =
            "Chart(data: \"p.csv\") {\n  let c = \"#111\"\n  Space(x * y) { Point(fill: c) }\n}";
        let state = state(text);
        let offset = text.find("let c").unwrap() + 4; // on the `c` of the decl
        assert!(renameable_at(&state, offset).is_some());
        let edit = rename_edits(&state, &uri(), offset, "color").expect("rename");
        let edits = &edit.changes.unwrap()[&uri()];
        // Declaration plus the one `fill: c` use are both rewritten.
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "color"));
    }
}
