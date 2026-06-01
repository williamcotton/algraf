//! Algebra Pratt-parser tests (spec §7.7, §12.5–12.6, §27.3–27.4).

use algraf_syntax::ast::{AlgebraExpr, AlgebraOp};
use algraf_syntax::parse_algebra;
use algraf_syntax::SyntaxNode;

/// Render the typed algebra tree as a fully parenthesized string so precedence
/// and associativity are visible. Parentheses from the source are transparent
/// here (the structure they produce is what matters).
fn shape(source: &str) -> String {
    let parse = parse_algebra(source);
    let expr = first_algebra(&parse.syntax()).expect("expected an algebra expression");
    fmt(&expr)
}

fn first_algebra(root: &SyntaxNode) -> Option<AlgebraExpr> {
    root.children().find_map(AlgebraExpr::cast)
}

fn fmt(expr: &AlgebraExpr) -> String {
    match expr {
        AlgebraExpr::Name(n) => {
            let name = n.name().unwrap_or_default();
            if n.is_quoted() {
                format!("`{name}`")
            } else {
                name
            }
        }
        AlgebraExpr::Call(c) => {
            let name = c.name().unwrap_or_default();
            let inner = c.inner().map(|e| fmt(&e)).unwrap_or_else(|| "<err>".into());
            format!("{name}({inner})")
        }
        AlgebraExpr::Binary(b) => {
            let op = b.op().map(AlgebraOp::symbol).unwrap_or("?");
            let lhs = b.lhs().map(|e| fmt(&e)).unwrap_or_else(|| "<err>".into());
            let rhs = b.rhs().map(|e| fmt(&e)).unwrap_or_else(|| "<err>".into());
            format!("({lhs} {op} {rhs})")
        }
        AlgebraExpr::Paren(p) => p.inner().map(|e| fmt(&e)).unwrap_or_else(|| "<err>".into()),
        AlgebraExpr::Error(_) => "<err>".into(),
    }
}

#[test]
fn test_single_identifier() {
    assert_eq!(shape("value"), "value");
    assert!(parse_algebra("value").diagnostics().is_empty());
}

#[test]
fn test_cross() {
    assert_eq!(
        shape("flipper_length * body_mass"),
        "(flipper_length * body_mass)"
    );
}

#[test]
fn test_nest_binds_tighter_than_cross() {
    // `quarter / type * amount` is `(quarter / type) * amount` (spec §8.6).
    assert_eq!(
        shape("quarter / type * amount"),
        "((quarter / type) * amount)"
    );
}

#[test]
fn test_cross_binds_tighter_than_blend() {
    // `time * lower + upper` parses as `(time * lower) + upper` (spec §8.6);
    // the analyzer (not the parser) later rejects the unparenthesized blend.
    assert_eq!(shape("time * lower + upper"), "((time * lower) + upper)");
}

#[test]
fn test_nest_is_left_associative() {
    assert_eq!(shape("a / b / c"), "((a / b) / c)");
}

#[test]
fn test_cross_is_left_associative() {
    assert_eq!(shape("a * b * c"), "((a * b) * c)");
}

#[test]
fn test_blend_is_left_associative() {
    assert_eq!(shape("(a + b + c)"), "((a + b) + c)");
}

#[test]
fn test_parentheses_override_precedence() {
    assert_eq!(
        shape("(quarter / type) * amount"),
        "((quarter / type) * amount)"
    );
    assert_eq!(shape("time * (lower + upper)"), "(time * (lower + upper))");
}

#[test]
fn test_nested_parentheses() {
    assert_eq!(shape("((a))"), "a");
    assert_eq!(shape("(a * (b / c))"), "(a * (b / c))");
}

#[test]
fn test_facet_expression() {
    assert_eq!(
        shape("(flipper_length * body_mass) / species"),
        "((flipper_length * body_mass) / species)"
    );
}

#[test]
fn test_removed_frame_call_shape_is_preserved_for_recovery() {
    assert_eq!(
        shape("transpose(group * value)"),
        "transpose((group * value))"
    );
    assert!(parse_algebra("transpose(group * value)")
        .diagnostics()
        .is_empty());
}

#[test]
fn test_removed_frame_call_shape_composes_with_nesting_for_rewrite() {
    assert_eq!(
        shape("transpose((group * value)) / region"),
        "(transpose((group * value)) / region)"
    );
}

#[test]
fn test_quoted_identifier_in_algebra() {
    let parse = parse_algebra("`flipper length` * `body mass (g)`");
    assert!(parse.diagnostics().is_empty());
    assert_eq!(
        shape("`flipper length` * `body mass`"),
        "(`flipper length` * `body mass`)"
    );
}

#[test]
fn test_quoted_transpose_is_column_name() {
    assert_eq!(shape("`transpose` * value"), "(`transpose` * value)");
}

#[test]
fn test_quoted_identifier_unescapes_backtick() {
    let parse = parse_algebra(r"`a\`b`");
    let expr = first_algebra(&parse.syntax()).unwrap();
    match expr {
        AlgebraExpr::Name(n) => {
            assert!(n.is_quoted());
            assert_eq!(n.name().as_deref(), Some("a`b"));
        }
        other => panic!("expected a name, got {other:?}"),
    }
}

#[test]
fn test_parse_is_lossless() {
    // The CST round-trips to the exact source, trivia included (spec §12.2).
    let source = "  quarter /  type * amount // trailing\n";
    let parse = parse_algebra(source);
    assert_eq!(parse.syntax().to_string(), source);
}

#[test]
fn test_comment_between_operands_is_preserved() {
    let source = "a /* not a block comment */ b";
    // `/*` lexes as slash then `*`; this still round-trips losslessly.
    let parse = parse_algebra(source);
    assert_eq!(parse.syntax().to_string(), source);
}

#[test]
fn test_missing_rhs_recovers() {
    let parse = parse_algebra("quarter /");
    // No panic; a diagnostic is produced and the tree is still navigable.
    assert!(parse.diagnostics().iter().any(|d| d.code == "E0009"));
    assert_eq!(shape("quarter /"), "(quarter / <err>)");
}

#[test]
fn test_missing_close_paren_recovers() {
    let parse = parse_algebra("(a / b");
    assert!(parse.diagnostics().iter().any(|d| d.code == "E0006"));
    // The inner expression is still recovered.
    assert_eq!(shape("(a / b"), "(a / b)");
}

#[test]
fn test_leading_operator_recovers() {
    let parse = parse_algebra("* a");
    assert!(parse.diagnostics().iter().any(|d| d.code == "E0009"));
    // Does not panic and yields a tree.
    assert!(first_algebra(&parse.syntax()).is_some());
}

#[test]
fn test_trailing_garbage_recovers() {
    let parse = parse_algebra("a b");
    assert!(parse.diagnostics().iter().any(|d| d.code == "E0011"));
    // Still lossless even with the trailing error node.
    assert_eq!(parse.syntax().to_string(), "a b");
}

#[test]
fn test_empty_input_recovers() {
    let parse = parse_algebra("");
    assert!(parse.diagnostics().iter().any(|d| d.code == "E0009"));
    assert!(first_algebra(&parse.syntax()).is_some());
}
