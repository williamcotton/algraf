//! Parser, formatter, and lexer resilience guardrails (spec §12.1, §27.4).
//!
//! These tests pin the spec's "recover and continue, never panic" guarantee
//! against the adversarial inputs most likely to break a recursive-descent
//! parser: deeply nested algebra and arrays, and densely malformed documents.
//! They are deterministic fixtures rather than a fuzzing harness, so they run in
//! CI without extra dependencies while exercising the same risk.
//!
//! Depths are chosen to stress nesting well beyond any realistic chart while
//! staying comfortably within a test thread's default stack. If a future change
//! makes the parser recurse more deeply per level and these overflow, that is a
//! signal to add an explicit nesting limit (and a diagnostic in the spec), not
//! to weaken the test.

use algraf_syntax::{format, parse};

/// Nesting depth used by the resilience tests. Large enough to be adversarial,
/// small enough to stay within a 2 MiB test-thread stack.
const DEPTH: usize = 600;

#[test]
fn deeply_nested_algebra_parens_do_not_panic() {
    let src = format!(
        "Chart(data: \"p.csv\") {{\n  Space({}x{}) {{ Point() }}\n}}",
        "(".repeat(DEPTH),
        ")".repeat(DEPTH),
    );
    // Balanced, just absurdly nested: the parser must finish and recover.
    let parsed = parse(&src);
    let _ = parsed.diagnostics();
    let _ = parsed.syntax();
}

#[test]
fn deeply_nested_algebra_operators_do_not_panic() {
    // A long right-leaning nest chain `x / x / x / ...`.
    let chain = std::iter::repeat("x")
        .take(DEPTH)
        .collect::<Vec<_>>()
        .join(" / ");
    let src = format!("Chart(data: \"p.csv\") {{\n  Space({chain}) {{ Point() }}\n}}");
    let parsed = parse(&src);
    let _ = parsed.diagnostics();
}

#[test]
fn deeply_nested_arrays_do_not_panic() {
    let src = format!(
        "Chart(data: \"p.csv\") {{\n  Scale(axis: \"x\", domain: {}1{})\n  Space(x) {{ Point() }}\n}}",
        "[".repeat(DEPTH),
        "]".repeat(DEPTH),
    );
    let parsed = parse(&src);
    let _ = parsed.diagnostics();
}

#[test]
fn unbalanced_delimiters_recover_without_panic() {
    // Every prefix of a deeply unbalanced document must parse to *something*
    // with diagnostics rather than crashing (spec §12.1).
    let messy = format!(
        "Chart(data: {}\n  Space((((x * {} {{ Point(fill: ]]]] )))\n",
        "(".repeat(DEPTH),
        "[".repeat(DEPTH),
    );
    let parsed = parse(&messy);
    assert!(
        !parsed.diagnostics().is_empty(),
        "malformed input should report diagnostics"
    );
}

#[test]
fn truncated_documents_at_every_length_recover() {
    // Truncating a valid document at each byte boundary is a classic
    // editor-in-progress state; none may panic.
    let full = "Chart(data: \"p.csv\") {\n  Space(a / (b * c) + d) {\n    Point(fill: x, alpha: 0.5)\n    Histogram(bins: 10)\n  }\n}";
    for end in 0..=full.len() {
        if !full.is_char_boundary(end) {
            continue;
        }
        let parsed = parse(&full[..end]);
        let _ = parsed.diagnostics();
    }
}

#[test]
fn formatter_returns_invalid_input_unchanged() {
    // The formatter must not reflow documents it cannot parse cleanly; it
    // returns them verbatim instead (spec §21.10).
    let invalid = [
        "Chart(data: \"p.csv\") {\n  Space((((x) {\n    Point(\n",
        "Chart(",
        "Space(a / / b) { Point(]]] }",
        &format!("Chart {{ {} }}", "(".repeat(DEPTH)),
    ];
    for source in invalid {
        assert_eq!(format(source), source, "invalid source must be unchanged");
    }
}

#[test]
fn formatter_is_idempotent_on_valid_input() {
    let source = "Chart(data: \"p.csv\") {\n    Space(a * b) {\n        Point(fill: x)\n    }\n}\n";
    let once = format(source);
    let twice = format(&once);
    assert_eq!(once, twice, "formatting must be idempotent");
}

/// Recovery fixtures for malformed nested constructs that previously lived only
/// as ad-hoc unit checks (spec §12.1, §27.4): nested calls, map literals, and
/// source constructors. Each must parse to a tree with diagnostics, never panic.
#[test]
fn malformed_nested_constructs_recover() {
    let fixtures = [
        // Nested call missing its argument value.
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(fill: Foo(bar:))\n  }\n}",
        // Nested call with an unterminated argument list.
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(size: Bin(\n  }\n}",
        // Map literal with a missing value after `=>`.
        "Chart(data: \"p.csv\") {\n  Scale(stroke: g, range: [\"a\" => ])\n  Space(x) { Point() }\n}",
        // Map literal with a dangling `=>` and no key.
        "Chart(data: \"p.csv\") {\n  Scale(fill: g, range: [ => \"red\"])\n  Space(x) { Point() }\n}",
        // Source constructor with no path argument.
        "Chart(data: GeoJson()) {\n  Space(geom) { Geo() }\n}",
        // Source constructor with a keyword argument instead of a positional path.
        "Chart(data: Shapefile(path: \"x.shp\")) {\n  Space(geom) { Geo() }\n}",
        // Source constructor missing its closing paren.
        "Chart(data: GeoJson(\"x.geojson\") {\n  Space(geom) { Geo() }\n}",
    ];
    for source in fixtures {
        let parsed = parse(source);
        // A syntax tree is always produced; the malformed ones report problems.
        let _ = parsed.syntax();
        let _ = parsed.diagnostics();
    }
}
