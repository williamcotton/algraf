use std::collections::HashSet;

use algraf_core::{codes, Diagnostic};

use crate::ast::{CallValue, LiteralKind, Root, ValueExpr};
use crate::source::{node_span, unescape_string_literal as string_value};
use crate::syntax_kind::SyntaxNode;

const KNOWN_FEATURE_GATES: &[&str] = &["sql", "network", "plugins", "experimental"];

pub(super) fn validate_source_header(root: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    let Some(root) = Root::cast(root.clone()) else {
        return;
    };
    let Some(header) = root.source_header() else {
        return;
    };

    let mut seen_args = HashSet::new();
    let mut saw_version = false;
    for arg in header.args() {
        let arg_span = node_span(arg.syntax());
        let Some(key) = arg.key() else {
            diagnostics.push(Diagnostic::error(
                codes::E0022,
                "Algraf source header arguments must be named",
                arg_span,
            ));
            continue;
        };
        if !seen_args.insert(key.clone()) {
            diagnostics.push(Diagnostic::error(
                codes::E0022,
                format!("duplicate Algraf source header argument `{key}`"),
                arg_span,
            ));
            continue;
        }

        match key.as_str() {
            "version" => {
                saw_version = true;
                validate_header_version(&arg, diagnostics);
            }
            "features" => validate_header_features(&arg, diagnostics),
            _ => diagnostics.push(Diagnostic::error(
                codes::E0022,
                format!("unsupported Algraf source header argument `{key}`"),
                arg_span,
            )),
        }
    }

    if !saw_version {
        diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`Algraf(...)` requires `version: \"0.21\"`",
            node_span(header.syntax()),
        ));
    }
}

fn validate_header_version(arg: &crate::ast::Arg, diagnostics: &mut Vec<Diagnostic>) {
    match arg.value() {
        Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
            let raw = string_value(&lit.text().unwrap_or_default());
            let span = node_span(lit.syntax());
            match parse_language_version(&raw) {
                Some((major, minor, _patch)) if major == 0 && minor <= 21 => {}
                Some(_) => diagnostics.push(Diagnostic::error(
                    codes::E0023,
                    format!("unsupported Algraf language version `{raw}`"),
                    span,
                )),
                None => diagnostics.push(Diagnostic::error(
                    codes::E0022,
                    "`version` expects a string like \"0.21\"",
                    span,
                )),
            }
        }
        Some(value) => diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`version` expects a string literal",
            node_span(value.syntax()),
        )),
        None => diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`version` expects a string literal",
            node_span(arg.syntax()),
        )),
    }
}

fn parse_language_version(raw: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = raw.split('.').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = if parts.len() == 3 {
        parts[2].parse().ok()?
    } else {
        0
    };
    Some((major, minor, patch))
}

fn validate_header_features(arg: &crate::ast::Arg, diagnostics: &mut Vec<Diagnostic>) {
    let Some(value) = arg.value() else {
        diagnostics.push(Diagnostic::error(
            codes::E1703,
            "`features` expects an array of string feature gates",
            node_span(arg.syntax()),
        ));
        return;
    };
    let ValueExpr::Array(array) = value else {
        diagnostics.push(Diagnostic::error(
            codes::E1703,
            "`features` expects an array of string feature gates",
            node_span(value.syntax()),
        ));
        return;
    };

    let mut seen = HashSet::new();
    for item in array.values() {
        let span = node_span(item.syntax());
        let Some(gate) = string_literal_value(&item) else {
            diagnostics.push(Diagnostic::error(
                codes::E1703,
                "`features` entries must be string literals",
                span,
            ));
            continue;
        };
        if !KNOWN_FEATURE_GATES.contains(&gate.as_str()) {
            diagnostics.push(Diagnostic::error(
                codes::E0024,
                format!("unknown feature gate `{gate}`"),
                span,
            ));
            continue;
        }
        if !seen.insert(gate.clone()) {
            diagnostics.push(Diagnostic::error(
                codes::E0024,
                format!("duplicate feature gate `{gate}`"),
                span,
            ));
        }
    }
}

pub(super) fn validate_gated_source_constructors(
    root: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let sql_enabled = source_version_at_least(root, 21) && source_has_feature(root, "sql");
    for call in root.descendants().filter_map(CallValue::cast) {
        if call.name().as_deref() == Some("Sqlite") && !sql_enabled {
            diagnostics.push(
                Diagnostic::error(
                    codes::E0025,
                    "`Sqlite(...)` requires Algraf version 0.21 and the `sql` feature gate",
                    node_span(call.syntax()),
                )
                .with_help(r#"add `Algraf(version: "0.21", features: ["sql"])` before the chart"#),
            );
        }
    }
}

fn source_version_at_least(root: &SyntaxNode, minor: u64) -> bool {
    source_header_arg(root, "version")
        .and_then(|arg| match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                let raw = string_value(&lit.text().unwrap_or_default());
                parse_language_version(&raw)
            }
            _ => None,
        })
        .is_some_and(|(major, declared_minor, _patch)| major == 0 && declared_minor >= minor)
}

fn source_has_feature(root: &SyntaxNode, feature: &str) -> bool {
    let Some(arg) = source_header_arg(root, "features") else {
        return false;
    };
    let Some(ValueExpr::Array(array)) = arg.value() else {
        return false;
    };
    array
        .values()
        .iter()
        .any(|item| string_literal_value(item).as_deref() == Some(feature))
}

fn source_header_arg(root: &SyntaxNode, key: &str) -> Option<crate::ast::Arg> {
    Root::cast(root.clone())?
        .source_header()?
        .args()
        .into_iter()
        .find(|arg| arg.key().as_deref() == Some(key))
}

fn string_literal_value(value: &ValueExpr) -> Option<String> {
    match value {
        ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
            Some(string_value(&lit.text().unwrap_or_default()))
        }
        _ => None,
    }
}
