use std::collections::{HashMap, HashSet};

use algraf_core::{codes, DiagnosticCode, Span};
use algraf_syntax::ast::{ChartItem, GeometryCall, Root, SpaceItem};
use algraf_syntax::{node_span, parse, tokenize, SyntaxKind};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Diagnostic, NumberOrString, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::completion::quote_identifier_if_needed;
use crate::document::DocumentState;
use crate::positions::{range_to_offsets, span_to_range};

pub fn code_actions_for(state: &DocumentState, params: CodeActionParams) -> CodeActionResponse {
    let uri = params.text_document.uri;
    let mut actions = Vec::new();
    for diagnostic in params.context.diagnostics {
        let Some(code) = diagnostic_code(&diagnostic) else {
            continue;
        };
        match code {
            codes::H3002 => {
                if let Some(action) =
                    quote_range_action(&uri, &state.text, &diagnostic, "Quote color literal")
                {
                    actions.push(action);
                }
            }
            codes::E1204 if diagnostic.message.contains("expects a quoted string value") => {
                if let Some(action) =
                    quote_range_action(&uri, &state.text, &diagnostic, "Quote string option")
                {
                    actions.push(action);
                }
            }
            codes::E1201 => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested geometry",
                            &uri,
                            range,
                            suggestion,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            codes::E1101 => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested column",
                            &uri,
                            range,
                            quote_identifier_if_needed(&suggestion),
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            codes::E1202 => {
                if let Some(suggestion) = extract_backtick_suggestion(&diagnostic.message) {
                    if let Some(range) = first_ident_range(&state.text, diagnostic.range) {
                        actions.push(edit_action(
                            "Use suggested property",
                            &uri,
                            range,
                            suggestion,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            codes::E1306 => {
                if let Some((start, end)) = range_to_offsets(&state.text, diagnostic.range) {
                    let text = state.text[start..end].trim();
                    let parts: Vec<_> = text.split('*').map(str::trim).collect();
                    if parts.len() == 3 && parts.iter().all(|part| !part.is_empty()) {
                        let replacement = format!("({} * {}) / {}", parts[0], parts[1], parts[2]);
                        actions.push(edit_action(
                            "Convert third Cartesian axis to nesting",
                            &uri,
                            diagnostic.range,
                            replacement,
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            codes::E1305 => {
                if let Some((start, end)) = range_to_offsets(&state.text, diagnostic.range) {
                    let text = state.text[start..end].trim();
                    if !text.starts_with('(') {
                        actions.push(edit_action(
                            "Parenthesize blend expression",
                            &uri,
                            diagnostic.range,
                            format!("({text})"),
                            diagnostic.clone(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(action) = histogram_refactor_action(&state.text, &uri, params.range) {
        actions.push(action);
    }
    actions
}

/// Offer a `refactor.rewrite` that desugars a single-`Histogram` space into the
/// explicit `Derive ... = Bin(...)` plus `Rect` form the analyzer produces
/// (spec §21.12). High-confidence only: fires when the space holds exactly one
/// `Histogram` over a single-column frame and is a direct chart-body item.
fn histogram_refactor_action(source: &str, uri: &Url, range: Range) -> Option<CodeActionOrCommand> {
    let (start, end) = range_to_offsets(source, range)?;
    let syntax = parse(source).syntax();
    let root = Root::cast(syntax)?;
    let chart = root.chart()?;

    for item in chart.items() {
        let ChartItem::Space(space) = item else {
            continue;
        };
        // Only desugar a space that is a direct chart-body item, so the new
        // `Derive` can be inserted as its sibling at the same indentation.
        if space.syntax().parent().map(|p| p.kind()) != Some(SyntaxKind::CHART_BLOCK) {
            continue;
        }
        let space_span = node_span(space.syntax());
        if end < space_span.start || start > space_span.end {
            continue;
        }

        // Exactly one geometry, a Histogram, and no scale/guide/theme items.
        let mut histogram: Option<GeometryCall> = None;
        let mut other_items = false;
        for child in space.items() {
            match child {
                SpaceItem::Geometry(call) if call.name().as_deref() == Some("Histogram") => {
                    if histogram.is_some() {
                        other_items = true;
                    } else {
                        histogram = Some(call);
                    }
                }
                SpaceItem::Error(_) => {}
                _ => other_items = true,
            }
        }
        let histogram = histogram?;
        if other_items {
            return None;
        }

        // The frame must be a single column identifier (the binned vector).
        let frame = space.frame()?;
        let input = match frame {
            algraf_syntax::ast::AlgebraExpr::Name(name) => name.raw_text()?,
            _ => return None,
        };

        let bin_args = collect_arg_text(&histogram, &["bins", "binWidth", "boundary", "closed"]);
        let rect_args = collect_arg_text(&histogram, &["fill", "stroke", "strokeWidth", "alpha"]);
        let derive_name = unique_derive_name(&chart, &input);

        let bin_call = if bin_args.is_empty() {
            format!("Bin({input})")
        } else {
            format!("Bin({input}, {})", bin_args.join(", "))
        };
        let rect_props = if rect_args.is_empty() {
            String::new()
        } else {
            format!(", {}", rect_args.join(", "))
        };

        let indent = line_indent(source, space_span.start);
        let replacement = format!(
            "Derive {derive_name} = {bin_call}\n\
             {indent}Space(bin_start * count, data: {derive_name}) {{\n\
             {indent}    Rect(xmin: bin_start, xmax: bin_end, ymax: count{rect_props})\n\
             {indent}}}"
        );

        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range: span_to_range(source, space_span),
                new_text: replacement,
            }],
        );
        return Some(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Desugar Histogram into Derive + Rect".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            ..CodeAction::default()
        }));
    }
    None
}

/// Render `key: value` fragments for the named args present on a geometry call,
/// preserving the source text of each value.
fn collect_arg_text(call: &GeometryCall, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in call.args() {
        let Some(key) = arg.key() else { continue };
        if !keys.contains(&key.as_str()) {
            continue;
        }
        if let Some(value) = arg.value() {
            let text = value.syntax().text().to_string();
            out.push(format!("{key}: {}", text.trim()));
        }
    }
    out
}

/// Pick a derived-table name that does not collide with an existing `Derive`.
fn unique_derive_name(chart: &algraf_syntax::ast::ChartBlock, input: &str) -> String {
    let base_root: String = input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let base_root = base_root.trim_matches('_');
    let base = if base_root.is_empty() {
        "binned".to_string()
    } else {
        format!("{base_root}_binned")
    };
    let existing: HashSet<String> = chart
        .items()
        .into_iter()
        .filter_map(|item| match item {
            ChartItem::Derive(decl) => decl.name(),
            _ => None,
        })
        .collect();
    if !existing.contains(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The whitespace prefix of the line that `offset` falls on.
fn line_indent(source: &str, offset: usize) -> String {
    let line_start = source[..offset.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    source[line_start..offset]
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

fn diagnostic_code(diagnostic: &Diagnostic) -> Option<DiagnosticCode> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => DiagnosticCode::parse(code),
        NumberOrString::Number(_) => None,
    }
}

fn quote_range_action(
    uri: &Url,
    source: &str,
    diagnostic: &Diagnostic,
    title: &str,
) -> Option<CodeActionOrCommand> {
    let range = first_ident_range(source, diagnostic.range).unwrap_or(diagnostic.range);
    let (start, end) = range_to_offsets(source, range)?;
    let text = source[start..end].trim();
    if text.is_empty() || text.starts_with('"') {
        return None;
    }
    Some(edit_action(
        title,
        uri,
        range,
        format!("{text:?}"),
        diagnostic.clone(),
    ))
}

fn edit_action(
    title: &str,
    uri: &Url,
    range: Range,
    new_text: impl Into<String>,
    diagnostic: Diagnostic,
) -> CodeActionOrCommand {
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range,
            new_text: new_text.into(),
        }],
    );
    CodeActionOrCommand::CodeAction(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        ..CodeAction::default()
    })
}

fn extract_backtick_suggestion(message: &str) -> Option<String> {
    let marker = "did you mean `";
    let start = message.find(marker)? + marker.len();
    let end = message[start..].find('`')?;
    Some(message[start..start + end].to_string())
}

fn first_ident_range(source: &str, range: Range) -> Option<Range> {
    let (start, end) = range_to_offsets(source, range)?;
    tokenize(&source[start..end])
        .tokens
        .into_iter()
        .find(|token| matches!(token.kind, algraf_syntax::TokenKind::Ident(_)))
        .map(|token| {
            let span = Span::new(start + token.span.start, start + token.span.end);
            span_to_range(source, span)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Url {
        Url::parse("file:///doc.ag").unwrap()
    }

    fn whole(source: &str) -> Range {
        span_to_range(source, Span::new(0, source.len()))
    }

    #[test]
    fn histogram_space_offers_derive_refactor() {
        let source =
            "Chart(data: \"p.csv\") {\n  Space(flipper_length) {\n    Histogram(bins: 20)\n  }\n}";
        let action = histogram_refactor_action(source, &uri(), whole(source));
        let action = action.expect("histogram refactor offered");
        match action {
            CodeActionOrCommand::CodeAction(action) => {
                assert!(action.title.to_lowercase().contains("derive"));
                assert!(action.edit.is_some());
            }
            _ => panic!("expected a code action"),
        }
    }

    #[test]
    fn non_histogram_space_offers_no_refactor() {
        let source = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        assert!(histogram_refactor_action(source, &uri(), whole(source)).is_none());
    }
}
