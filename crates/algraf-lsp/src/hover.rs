use algraf_data::DataType;
use algraf_semantics::registry;
use algraf_syntax::{tokenize, unescape_string_literal};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::document::DocumentState;
use crate::positions::span_to_range;

pub(crate) fn hover_at(state: &DocumentState, offset: usize) -> Option<Hover> {
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
        TokenKind::QuotedIdent(raw) => hover_for_ident(state, &tokens, idx, raw),
        TokenKind::String(raw) => hover_for_string(state, raw),
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
    if next_significant_is_colon(tokens, idx) {
        return Some(format!(
            "**Property `{name}`**\n\n{}",
            registry::property_doc(name)
        ));
    }
    if registry::geometry(name).is_some() {
        return Some(format!(
            "**Geometry `{name}`**\n\n{}",
            registry::geometry_doc(name)
        ));
    }
    if let Some(column) = state
        .primary_schema
        .as_ref()
        .and_then(|schema| schema.iter().find(|column| column.name == name))
    {
        let source = state
            .data_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "CSV schema sample".to_string());
        let examples = if column.examples.is_empty() {
            String::new()
        } else {
            format!("\n\nExamples: `{}`", column.examples.join("`, `"))
        };
        return Some(format!(
            "**Column `{}`**\n\nType: `{}`\n\nSource: {} (provisional LSP sample){}",
            column.name,
            dtype_name(column.dtype),
            source,
            examples
        ));
    }
    None
}

fn hover_for_string(state: &DocumentState, raw: &str) -> Option<String> {
    let value = unescape_string_literal(raw);
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
    if state.data_path.is_some() {
        None
    } else {
        Some("String literal".to_string())
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

fn operator_hover(title: &str, body: &str) -> String {
    format!("**{title}**\n\n{body}")
}

pub(crate) fn dtype_name(dtype: DataType) -> &'static str {
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
