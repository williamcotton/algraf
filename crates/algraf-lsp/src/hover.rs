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
        return Some(format!(
            "**Feature gate `{value}`**\n\nReserved v0.20 feature gate; it does not enable runtime access yet."
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
            data_path: None,
        }
    }

    fn markdown(hover: Hover) -> String {
        match hover.contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected markup"),
        }
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
}
