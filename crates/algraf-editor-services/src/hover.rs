use algraf_core::Span;
use algraf_data::{ColumnDef, DataType};
use algraf_semantics::ir::ColumnDefIr;
use algraf_semantics::registry::{self, Accept, ArgDoc, CallDoc, GeometryDef, PropSpec};
use algraf_syntax::ast::{
    AlgebraExpr, Arg, ChartItem, DeriveDecl, LiteralKind, Root, SpaceBlock, ValueExpr,
};
use algraf_syntax::{
    node_span, parse, source_constructor_meta, tokenize, unescape_quoted_ident,
    unescape_string_literal, SyntaxKind, SyntaxNode,
};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::document::{DocumentState, SourcePreview};
use crate::positions::span_to_range;

pub fn hover_at(state: &DocumentState, offset: usize) -> Option<Hover> {
    let lexed = tokenize(&state.text);
    let tokens: Vec<_> = lexed
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let idx = tokens
        .iter()
        .position(|token| token.span.contains(offset))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.span.end == offset && token.span.start < token.span.end)
        })?;
    let token = &tokens[idx];

    use algraf_syntax::TokenKind;
    let contents = match &token.kind {
        TokenKind::Star => Some(operator_hover(
            "Cross operator",
            "`*` builds a Cartesian product between algebraic frames.",
        )),
        TokenKind::Slash => Some(operator_hover(
            "Nest operator",
            "`/` nests the right frame inside each value of the left frame.",
        )),
        TokenKind::Plus => Some(operator_hover(
            "Blend operator",
            "`+` unions compatible domains in an explicitly parenthesized blend.",
        )),
        TokenKind::Ident(name) => hover_for_ident(state, &tokens, idx, name),
        TokenKind::QuotedIdent(raw) => {
            let name = unescape_quoted_ident(raw);
            hover_for_ident(state, &tokens, idx, &name)
        }
        TokenKind::String(raw) => hover_for_string(state, token.span, raw),
        _ => None,
    }?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: contents,
        }),
        range: Some(span_to_range(&state.text, token.span)),
    })
}

fn hover_for_ident(
    state: &DocumentState,
    tokens: &[algraf_syntax::TokenWithSpan],
    idx: usize,
    name: &str,
) -> Option<String> {
    let span = tokens[idx].span;
    if let Some(target) = derived_hover_target_at(&state.text, span.start) {
        return Some(derived_table_hover(state, target));
    }
    if let Some(context) = call_name_context_at(&state.text, span.start) {
        match context {
            CallNameContext::Declaration => {
                if let Some(doc) = registry::declaration_doc(name) {
                    return Some(declaration_hover(doc));
                }
            }
            CallNameContext::Geometry => {
                if let Some(def) = registry::geometry(name) {
                    return Some(geometry_hover(def));
                }
            }
            CallNameContext::Stat => {
                if is_stat_name(name) {
                    return Some(stat_hover(name));
                }
            }
        }
    }
    if next_significant_is_colon(tokens, idx) {
        return Some(format!(
            "**Property {}**\n\n{}",
            inline_code(name),
            registry::property_doc(name)
        ));
    }
    if next_significant_is_lparen(tokens, idx) && is_stat_name(name) {
        return Some(stat_hover(name));
    }
    if let Some(def) = registry::geometry(name) {
        return Some(geometry_hover(def));
    }
    if let Some(doc) = registry::declaration_doc(name) {
        return Some(declaration_hover(doc));
    }
    if name == "transpose" && next_significant_is_lparen(tokens, idx) {
        return Some(
            "**Frame operator `transpose`**\n\nSwaps the two axes of a two-dimensional Cartesian frame."
                .to_string(),
        );
    }
    if let Some(meta) = source_constructor_meta(name) {
        if meta.name != "Sqlite" || (state.text.contains("0.21") && state.text.contains("\"sql\""))
        {
            return Some(format!("**Source {}**\n\n{}", inline_code(name), meta.doc));
        }
    }
    match name {
        "Algraf" => {
            return Some(
                "**Algraf source header**\n\nDeclares the source language version and optional feature gates."
                    .to_string(),
            );
        }
        "Style" => {
            return Some(
                "**Style fragment**\n\nReusable property bag applied with `style:`.".to_string(),
            );
        }
        "Stop" => {
            return Some(
                "**Gradient stop**\n\nA positioned continuous color stop with `value:` and `color:`."
                    .to_string(),
            );
        }
        _ => {}
    }
    if let Some(column) = column_hover_at(state, span.start, name) {
        return Some(column);
    }
    None
}

fn is_stat_name(name: &str) -> bool {
    matches!(
        name,
        "Bin"
            | "Smooth"
            | "Bin2D"
            | "HexBin"
            | "ContourLines"
            | "ContourBands"
            | "Density2D"
            | "Density2DContours"
            | "Density2DBands"
            | "Summary2D"
            | "SummaryHex"
            | "StepVertices"
            | "VectorEndpoints"
            | "CurveSample"
            | "IntervalSegments"
            | "IntervalRects"
            | "IntervalMiddles"
            | "Centroid"
            | "Simplify"
            | "SpatialJoin"
            | "Count"
    )
}

enum CallNameContext {
    Declaration,
    Geometry,
    Stat,
}

fn call_name_context_at(text: &str, offset: usize) -> Option<CallNameContext> {
    let root = parse(text).syntax();
    for node in root.descendants() {
        let context = match node.kind() {
            SyntaxKind::CHART_BLOCK
            | SyntaxKind::SPACE_BLOCK
            | SyntaxKind::SCALE_DECL
            | SyntaxKind::GUIDE_DECL
            | SyntaxKind::THEME_DECL
            | SyntaxKind::LAYOUT_DECL
            | SyntaxKind::TABLE_DECL => CallNameContext::Declaration,
            SyntaxKind::GEOMETRY_CALL => CallNameContext::Geometry,
            SyntaxKind::STAT_CALL => CallNameContext::Stat,
            _ => continue,
        };
        let Some(span) = first_code_token_span(&node) else {
            continue;
        };
        if span.contains(offset) {
            return Some(context);
        }
    }
    None
}

fn first_code_token_span(node: &SyntaxNode) -> Option<Span> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
        .map(|token| {
            let range = token.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
}

struct DerivedHoverTarget {
    name: String,
    producer: Option<String>,
    context: DerivedHoverContext,
}

enum DerivedHoverContext {
    Declaration,
    Reference,
}

fn derived_hover_target_at(text: &str, offset: usize) -> Option<DerivedHoverTarget> {
    let root = parse(text).syntax();
    let mut producers = std::collections::HashMap::new();
    for node in root.descendants() {
        if node.kind() != SyntaxKind::DERIVE_DECL {
            continue;
        }
        let Some(decl) = DeriveDecl::cast(node.clone()) else {
            continue;
        };
        let Some(name) = decl.name() else { continue };
        let producer = decl
            .stat()
            .map(|stat| stat.syntax().text().to_string().trim().to_string());
        producers.insert(name, producer);
    }

    for node in root.descendants() {
        if node.kind() == SyntaxKind::DERIVE_DECL {
            let Some(decl) = DeriveDecl::cast(node.clone()) else {
                continue;
            };
            let (Some(name), Some(span)) = (decl.name(), derive_name_span(&node)) else {
                continue;
            };
            if span.contains(offset) {
                return Some(DerivedHoverTarget {
                    producer: producers.get(&name).and_then(Clone::clone),
                    name,
                    context: DerivedHoverContext::Declaration,
                });
            }
        }

        if node.kind() != SyntaxKind::ALGEBRA_NAME || !is_data_arg_value(&node) {
            continue;
        }
        let Some(name_expr) = algraf_syntax::ast::AlgebraName::cast(node.clone()) else {
            continue;
        };
        let (Some(name), Some(span)) = (name_expr.name(), name_expr.ident_span()) else {
            continue;
        };
        if span.contains(offset) && producers.contains_key(&name) {
            return Some(DerivedHoverTarget {
                producer: producers.get(&name).and_then(Clone::clone),
                name,
                context: DerivedHoverContext::Reference,
            });
        }
    }
    None
}

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

fn derived_table_hover(state: &DocumentState, target: DerivedHoverTarget) -> String {
    let table = state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .and_then(|ir| {
            ir.derived_tables
                .iter()
                .find(|table| table.name == target.name)
        });
    let title = format!("**Derived table {}**", inline_code(&target.name));
    let producer = target
        .producer
        .as_deref()
        .or_else(|| table.map(|table| table.stat.kind.display_name()))
        .unwrap_or("stat");
    let context = match target.context {
        DerivedHoverContext::Declaration => String::new(),
        DerivedHoverContext::Reference => {
            "\n\nThis `Space` is bound to the derived table.".to_string()
        }
    };

    match table {
        Some(table) if !table.output_schema.is_empty() => format!(
            "{title}\n\nProduced by {}.{context}\n\nColumns:\n\n{}",
            inline_code(producer),
            derived_schema_table(&table.output_schema)
        ),
        _ => format!(
            "{title}\n\nProduced by {}.{context}\n\nDerived output schema is unavailable for this incomplete or invalid analysis.",
            inline_code(producer)
        ),
    }
}

fn column_hover_at(state: &DocumentState, offset: usize, name: &str) -> Option<String> {
    if let Some(table_name) = space_data_table_at(&state.text, offset) {
        if let Some(column) = derived_column(state, &table_name, name) {
            return Some(format!(
                "**Column {}**\n\nType: `{}`\n\nSource: Derived table {} (provisional LSP sample)",
                inline_code(&column.name),
                dtype_name(column.dtype),
                inline_code(&table_name)
            ));
        }
        if let Some(column) = state
            .table_schemas
            .get(&table_name)
            .and_then(|schema| schema.iter().find(|column| column.name == name))
        {
            return Some(data_column_hover(
                column,
                &format!("Table {}", inline_code(&table_name)),
            ));
        }
    }
    let source = state
        .data_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "CSV schema sample".to_string());
    state
        .primary_schema
        .as_deref()
        .and_then(|schema| schema.iter().find(|column| column.name == name))
        .map(|column| data_column_hover(column, &source))
}

fn derived_column<'a>(
    state: &'a DocumentState,
    table_name: &str,
    column_name: &str,
) -> Option<&'a ColumnDefIr> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .and_then(|ir| {
            ir.derived_tables
                .iter()
                .find(|table| table.name == table_name)
        })
        .and_then(|table| {
            table
                .output_schema
                .iter()
                .find(|column| column.name == column_name)
        })
}

fn data_column_hover(column: &ColumnDef, source: &str) -> String {
    let examples = if column.examples.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nExamples: {}",
            column
                .examples
                .iter()
                .map(|example| inline_code(example))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "**Column {}**\n\nType: `{}`\n\nSource: {} (provisional LSP sample){}",
        inline_code(&column.name),
        dtype_name(column.dtype),
        source,
        examples
    )
}

fn space_data_table_at(text: &str, offset: usize) -> Option<String> {
    let root = parse(text).syntax();
    let space = root
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::SPACE_BLOCK)
        .find(|node| node_span(node).contains(offset))?;
    let block = SpaceBlock::cast(space)?;
    for arg in block.args() {
        if arg.key().as_deref() != Some("data") {
            continue;
        }
        let Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) = arg.value() else {
            continue;
        };
        return name.name();
    }
    None
}

fn hover_for_string(state: &DocumentState, span: Span, raw: &str) -> Option<String> {
    let value = unescape_string_literal(raw);
    if let Some(target) = source_hover_target_at(&state.text, span.start) {
        return Some(source_preview_hover(state, target));
    }
    if ["identity", "stack", "fill"].contains(&value.as_str()) {
        return Some(format!(
            "**Bar layout `{value}`**\n\nControls how bars resolve collisions in a shared space."
        ));
    }
    if ["lm", "loess"].contains(&value.as_str()) {
        return Some(format!(
            "**Smooth method `{value}`**\n\nControls the statistical model used for a smooth layer."
        ));
    }
    if ["iso-date", "iso-minute"].contains(&value.as_str()) {
        return Some(format!(
            "**Temporal format `{value}`**\n\nDeterministic ISO-style axis label format."
        ));
    }
    if ["minute", "hour", "day", "week", "month", "quarter", "year"].contains(&value.as_str()) {
        return Some(format!(
            "**Temporal interval `{value}`**\n\nCalendar-aware temporal bin interval."
        ));
    }
    if ["sql", "network", "plugins", "experimental"].contains(&value.as_str()) {
        let body = if value == "sql" {
            "Enables local `Sqlite(...)` sources in Algraf v0.21."
        } else {
            "Reserved feature gate; it does not enable runtime access yet."
        };
        return Some(format!("**Feature gate `{value}`**\n\n{body}"));
    }
    if state.data_path.is_some() {
        None
    } else {
        Some("String literal".to_string())
    }
}

enum SourceHoverTarget {
    Primary { label: String },
    Table { name: String, label: String },
}

fn source_hover_target_at(text: &str, offset: usize) -> Option<SourceHoverTarget> {
    let root = Root::cast(parse(text).syntax())?;
    for chart in root.charts() {
        for arg in chart.args() {
            if arg.key().as_deref() != Some("data") {
                continue;
            }
            let Some((label, span)) = string_literal_value_span(arg.value()) else {
                continue;
            };
            if span.contains(offset) {
                return Some(SourceHoverTarget::Primary { label });
            }
        }
        for item in chart.items() {
            let ChartItem::Table(decl) = item else {
                continue;
            };
            let Some(name) = decl.name() else { continue };
            let Some((label, span)) = string_literal_value_span(decl.source()) else {
                continue;
            };
            if span.contains(offset) {
                return Some(SourceHoverTarget::Table { name, label });
            }
        }
    }
    None
}

fn string_literal_value_span(value: Option<ValueExpr>) -> Option<(String, Span)> {
    let ValueExpr::Literal(lit) = value? else {
        return None;
    };
    if lit.kind() != Some(LiteralKind::String) {
        return None;
    }
    Some((
        unescape_string_literal(&lit.text().unwrap_or_default()),
        lit.token_span().unwrap_or_else(|| node_span(lit.syntax())),
    ))
}

fn source_preview_hover(state: &DocumentState, target: SourceHoverTarget) -> String {
    match target {
        SourceHoverTarget::Primary { label } => {
            if let Some(preview) = state.source_previews.primary.as_ref() {
                source_preview_markdown(&label, None, preview)
            } else if let Some(schema) = state.primary_schema.as_deref() {
                source_schema_markdown(&label, None, schema)
            } else {
                source_unavailable_markdown(&label, None)
            }
        }
        SourceHoverTarget::Table { name, label } => {
            if let Some(preview) = state.source_previews.tables.get(&name) {
                source_preview_markdown(&label, Some(&name), preview)
            } else if let Some(schema) = state.table_schemas.get(&name) {
                source_schema_markdown(&label, Some(&name), schema)
            } else {
                source_unavailable_markdown(&label, Some(&name))
            }
        }
    }
}

fn source_preview_markdown(
    label: &str,
    table_name: Option<&str>,
    preview: &SourcePreview,
) -> String {
    let mut out = source_title(label, table_name);
    if !preview.label.is_empty() && preview.label != label {
        out.push_str("\n\nResolved path: ");
        out.push_str(&inline_code(&preview.label));
    }
    out.push_str("\n\nSampled schema:\n\n");
    out.push_str(&source_schema_table(&preview.schema));
    if !preview.rows.is_empty() && !preview.row_headers.is_empty() {
        out.push_str("\n\nSample rows:\n\n");
        out.push_str(&sample_rows_table(&preview.row_headers, &preview.rows));
    } else {
        out.push_str("\n\nSample rows unavailable.");
    }
    out.push_str("\n\nProvisional LSP sample.");
    out
}

fn source_schema_markdown(label: &str, table_name: Option<&str>, schema: &[ColumnDef]) -> String {
    let mut out = source_title(label, table_name);
    out.push_str("\n\nSampled schema:\n\n");
    out.push_str(&source_schema_table(schema));
    out.push_str("\n\nSample rows unavailable.");
    out.push_str("\n\nProvisional LSP sample.");
    out
}

fn source_unavailable_markdown(label: &str, table_name: Option<&str>) -> String {
    let mut out = source_title(label, table_name);
    out.push_str(
        "\n\nSource preview unavailable. Diagnostics, if any, report the specific load problem.",
    );
    out
}

fn source_title(label: &str, table_name: Option<&str>) -> String {
    let mut out = format!("**Data source {}**", inline_code(label));
    if let Some(name) = table_name {
        out.push_str("\n\nTable: ");
        out.push_str(&inline_code(name));
    }
    out
}

fn declaration_hover(doc: CallDoc) -> String {
    let mut out = format!(
        "**{} {}**\n\n{}",
        doc.kind,
        inline_code(doc.name),
        doc.description
    );
    if !doc.args.is_empty() {
        out.push_str("\n\nAttributes:\n\n");
        out.push_str(&arg_doc_table(doc.args));
    }
    if !doc.example.is_empty() {
        out.push_str("\n\nExample:\n\n```ag\n");
        out.push_str(doc.example);
        out.push_str("\n```");
    }
    out
}

fn geometry_hover(def: &GeometryDef) -> String {
    let mut out = format!(
        "**Geometry {}**\n\n{}",
        inline_code(def.name),
        registry::geometry_doc(def.name)
    );
    if !def.props.is_empty() {
        out.push_str("\n\nProperties:\n\n");
        out.push_str(&prop_doc_table(def.props));
    }
    let example = registry::geometry_example(def.name);
    if !example.is_empty() {
        out.push_str("\n\nExample:\n\n```ag\n");
        out.push_str(example);
        out.push_str("\n```");
    }
    out
}

fn stat_hover(name: &str) -> String {
    let mut out = format!(
        "**Stat {}**\n\n{}",
        inline_code(name),
        registry::stat_doc(name)
    );
    let args = registry::declaration_arg_names(name);
    if !args.is_empty() {
        let rows = args
            .iter()
            .map(|arg| {
                vec![
                    (*arg).to_string(),
                    stat_arg_value_hint(name, arg).to_string(),
                    registry::property_doc(arg).to_string(),
                ]
            })
            .collect::<Vec<_>>();
        out.push_str("\n\nArguments:\n\n");
        out.push_str(&markdown_table(
            &["Argument", "Value", "Description"],
            &rows,
        ));
    }
    let example = registry::stat_example(name);
    if !example.is_empty() {
        out.push_str("\n\nExample:\n\n```ag\n");
        out.push_str(example);
        out.push_str("\n```");
    }
    out
}

fn stat_arg_value_hint(stat: &str, arg: &str) -> &'static str {
    match arg {
        "method" => "\"lm\" | \"loess\"",
        "closed" => "\"left\" | \"right\"",
        "interval" => {
            "\"minute\" | \"hour\" | \"day\" | \"week\" | \"month\" | \"quarter\" | \"year\""
        }
        "direction" if stat == "StepVertices" => "\"hv\" | \"vh\"",
        "orientation" => "\"vertical\" | \"horizontal\"",
        "reducer" => "\"count\" | \"mean\" | \"min\" | \"max\" | \"sum\" | \"median\"",
        "se" => "boolean",
        "table" => "table name",
        "z" => "column",
        "bins" | "binWidth" | "boundary" | "span" | "bandwidth" | "grid" | "levels"
        | "lengthScale" | "curvature" | "points" | "capWidth" | "width" | "tolerance" => "number",
        _ => "argument",
    }
}

fn arg_doc_table(args: &[ArgDoc]) -> String {
    let rows = args
        .iter()
        .map(|arg| {
            vec![
                arg.name.to_string(),
                arg.value.to_string(),
                arg.default.unwrap_or("").to_string(),
                arg.doc.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    markdown_table(&["Attribute", "Value", "Default", "Description"], &rows)
}

fn prop_doc_table(props: &[PropSpec]) -> String {
    let rows = props
        .iter()
        .map(|prop| {
            vec![
                prop.name.to_string(),
                accepts_label(prop.accepts),
                if prop.required { "yes" } else { "" }.to_string(),
                registry::property_doc(prop.name).to_string(),
            ]
        })
        .collect::<Vec<_>>();
    markdown_table(&["Property", "Value", "Required", "Description"], &rows)
}

fn accepts_label(accepts: &[Accept]) -> String {
    accepts
        .iter()
        .map(|accept| match accept {
            Accept::Column => "column".to_string(),
            Accept::Number => "number".to_string(),
            Accept::Color => "color".to_string(),
            Accept::Str => "string".to_string(),
            Accept::Bool => "boolean".to_string(),
            Accept::Enum(values) => values
                .iter()
                .map(|value| format!("\"{value}\""))
                .collect::<Vec<_>>()
                .join(" | "),
            Accept::NumberArray => "number[]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn source_schema_table(schema: &[ColumnDef]) -> String {
    let rows = schema
        .iter()
        .map(|column| {
            vec![
                column.name.clone(),
                dtype_name(column.dtype).to_string(),
                column.examples.join(", "),
            ]
        })
        .collect::<Vec<_>>();
    markdown_table(&["Column", "Type", "Examples"], &rows)
}

fn derived_schema_table(schema: &[ColumnDefIr]) -> String {
    let rows = schema
        .iter()
        .map(|column| vec![column.name.clone(), dtype_name(column.dtype).to_string()])
        .collect::<Vec<_>>();
    markdown_table(&["Column", "Type"], &rows)
}

fn sample_rows_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let display_columns = headers.len().min(8);
    let display_headers = headers[..display_columns]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let display_rows = rows
        .iter()
        .map(|row| {
            (0..display_columns)
                .map(|idx| row.get(idx).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    markdown_table(&display_headers, &display_rows)
}

fn markdown_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(
        &headers
            .iter()
            .map(|header| table_cell(header))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    out.push_str(" |\n| ");
    out.push_str(
        &headers
            .iter()
            .map(|_| "---".to_string())
            .collect::<Vec<_>>()
            .join(" | "),
    );
    out.push_str(" |\n");
    for row in rows {
        out.push_str("| ");
        out.push_str(
            &(0..headers.len())
                .map(|idx| table_cell(row.get(idx).map(String::as_str).unwrap_or("")))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        out.push_str(" |\n");
    }
    out.trim_end().to_string()
}

fn table_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\n', '\r'], " ")
}

fn inline_code(value: &str) -> String {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for ch in value.chars() {
        if ch == '`' {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "`".repeat(max_run + 1);
    if value.starts_with('`') || value.ends_with('`') {
        format!("{fence} {value} {fence}")
    } else {
        format!("{fence}{value}{fence}")
    }
}

fn next_significant_is_colon(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Eof))
        .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
}

fn next_significant_is_lparen(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Eof))
        .is_some_and(|token| matches!(token.kind, TokenKind::LParen))
}

fn operator_hover(title: &str, body: &str) -> String {
    format!("**{title}**\n\n{body}")
}

pub fn dtype_name(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Geometry => "geometry",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{AnalysisState, DocumentState};
    use algraf_semantics::analyze_with_tables;
    use std::collections::HashMap;

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

    fn markdown(hover: Hover) -> String {
        match hover.contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected markup"),
        }
    }

    fn range(hover: Hover) -> lsp_types::Range {
        hover.range.expect("hover range")
    }

    fn col(name: &str, dtype: DataType, examples: &[&str]) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            dtype,
            nullable: false,
            examples: examples
                .iter()
                .map(|example| (*example).to_string())
                .collect(),
        }
    }

    fn analyzed_state(text: &str, schema: Vec<ColumnDef>) -> DocumentState {
        let analysis = analyze_with_tables(&parse(text).syntax(), &schema, &HashMap::new());
        let mut state = state(text);
        state.analysis = Some(AnalysisState {
            ir: analysis.ir,
            diagnostics: analysis.diagnostics,
        });
        state.primary_schema = Some(schema);
        state
    }

    #[test]
    fn hovers_geometry_name_with_registry_doc() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        let offset = text.find("Point").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Geometry `Point`"));
    }

    #[test]
    fn hovers_property_key_with_registry_doc() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(fill: x)\n  }\n}";
        let offset = text.find("fill").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Property `fill`"));
    }

    #[test]
    fn hovers_cross_operator() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        let offset = text.find('*').unwrap();
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Cross operator"));
    }

    #[test]
    fn hovers_transpose_frame_operator() {
        let text = "Chart(data: \"p.csv\") {\n  Space(transpose(x * y)) {\n    Point()\n  }\n}";
        let offset = text.find("transpose").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Frame operator `transpose`"));
    }

    #[test]
    fn hovers_source_constructor_with_shared_docs() {
        let text = "Chart(data: GeoJson(\"map.geojson\")) {\n  Space() { Geo() }\n}";
        let offset = text.find("GeoJson").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Source `GeoJson`"));
    }

    #[test]
    fn hovers_primary_column_type_and_examples() {
        let text = "Chart(data: \"p.csv\") {\n  Space(region * sales) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("sales", DataType::Float, &["10.5", "12"])]);
        let offset = text.find("sales").unwrap() + 1;
        let md = markdown(hover_at(&state, offset).expect("hover"));
        assert!(md.contains("Column `sales`"));
        assert!(md.contains("Type: `float`"));
        assert!(md.contains("Examples: `10.5`, `12`"));
    }

    #[test]
    fn hovers_non_ascii_primary_column_with_utf16_range() {
        let text = "Chart(data: \"p.csv\") {\n  Space(`café` * mass) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("café", DataType::Float, &["1.5"])]);
        let offset = text.find("café").unwrap() + "caf".len();
        let hover = hover_at(&state, offset).expect("hover");
        let md = markdown(hover.clone());
        assert!(md.contains("Column `café`"));
        let range = range(hover);
        assert_eq!(
            range.start,
            crate::positions::offset_to_position(text, text.find('`').unwrap())
        );
        assert_eq!(range.end.character - range.start.character, 6);
    }

    #[test]
    fn hovers_named_table_column_type_and_examples() {
        let text = "Chart(data: \"p.csv\") {\n  Table cities = \"cities.csv\"\n  Space(lat * lon, data: cities) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("lat", DataType::String, &["wrong"])]);
        state.table_schemas.insert(
            "cities".to_string(),
            vec![col("lat", DataType::Float, &["45.1"])],
        );
        let offset = text.find("lat *").unwrap() + 1;
        let md = markdown(hover_at(&state, offset).expect("hover"));
        assert!(md.contains("Column `lat`"));
        assert!(md.contains("Type: `float`"));
        assert!(md.contains("Source: Table `cities`"));
        assert!(md.contains("Examples: `45.1`"));
    }

    #[test]
    fn hovers_derived_table_declaration_schema() {
        let text = "Chart(data: \"p.csv\") {\n  Derive binned = Bin2D(x, y, bins: 10)\n  Space(x_center * y_center, data: binned) {\n    Rect(xmin: x_min, xmax: x_max, ymin: y_min, ymax: y_max)\n  }\n}";
        let state = analyzed_state(
            text,
            vec![
                col("x", DataType::Float, &[]),
                col("y", DataType::Float, &[]),
            ],
        );
        let offset = text.find("binned").unwrap() + 1;
        let md = markdown(hover_at(&state, offset).expect("hover"));
        assert!(md.contains("Derived table `binned`"), "{md}");
        assert!(md.contains("Produced by `Bin2D(x, y, bins: 10)`"), "{md}");
        assert!(md.contains("| x_start | float |"), "{md}");
        assert!(md.contains("| count | integer |"), "{md}");
    }

    #[test]
    fn hovers_derived_table_reference_and_columns() {
        let text = "Chart(data: \"p.csv\") {\n  Derive binned = Bin2D(x, y, bins: 10)\n  Derive trend = Smooth(x_center, y_center, method: \"lm\")\n  Space(x * y, data: trend) { Line() }\n  Space(x_center * y_center, data: binned) { Point() }\n}";
        let state = analyzed_state(
            text,
            vec![
                col("x", DataType::Float, &[]),
                col("y", DataType::Float, &[]),
            ],
        );
        let trend_offset = text.find("data: trend").unwrap() + "data: ".len();
        let trend = markdown(hover_at(&state, trend_offset).expect("hover"));
        assert!(trend.contains("Derived table `trend`"), "{trend}");
        assert!(
            trend.contains("Produced by `Smooth(x_center, y_center, method: \"lm\")`"),
            "{trend}"
        );
        assert!(trend.contains("| x | float |"), "{trend}");
        assert!(trend.contains("| y | float |"), "{trend}");

        let x_center_offset = text.rfind("x_center *").unwrap() + 1;
        let column = markdown(hover_at(&state, x_center_offset).expect("hover"));
        assert!(column.contains("Column `x_center`"), "{column}");
        assert!(
            column.contains("Source: Derived table `binned`"),
            "{column}"
        );
    }

    #[test]
    fn hovers_declaration_and_rect_calls_with_attributes_and_examples() {
        let text = "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\")\n  Space(x * y) {\n    Rect(xmin: x0, xmax: x1, ymin: y0, ymax: y1)\n  }\n}";
        let chart = markdown(hover_at(&state(text), text.find("Chart").unwrap()).expect("hover"));
        assert!(chart.contains("Declaration `Chart`"), "{chart}");
        assert!(chart.contains("| data | source |"), "{chart}");
        assert!(chart.contains("```ag"), "{chart}");

        let theme = markdown(hover_at(&state(text), text.find("Theme").unwrap()).expect("hover"));
        assert!(theme.contains("Declaration `Theme`"), "{theme}");
        assert!(theme.contains("\"minimal\""), "{theme}");
        assert!(theme.contains("grid"), "{theme}");

        let rect = markdown(hover_at(&state(text), text.find("Rect").unwrap()).expect("hover"));
        assert!(rect.contains("Geometry `Rect`"), "{rect}");
        assert!(rect.contains("| xmin |"), "{rect}");
        assert!(rect.contains("column \\| number"), "{rect}");
        assert!(rect.contains("Rect(xmin: x_min"), "{rect}");
    }
}
