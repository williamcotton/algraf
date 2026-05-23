//! Block grammar parser tests (spec §7, §12.7–12.14, §27.3–27.4).

use algraf_syntax::ast::{ChartItem, LiteralKind, Root, SpaceItem, ValueExpr};
use algraf_syntax::{parse, SyntaxNode};

fn root(source: &str) -> Root {
    Root::cast(parse(source).syntax()).expect("root node")
}

fn no_errors(source: &str) {
    let parsed = parse(source);
    assert!(
        parsed.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        parsed.diagnostics()
    );
}

#[test]
fn test_minimal_chart() {
    let source = r#"Chart(data: "penguins.csv") {
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.7, size: 3)
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let args = chart.args();
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].key().as_deref(), Some("data"));

    let items = chart.items();
    assert_eq!(items.len(), 1);
    let ChartItem::Space(space) = &items[0] else {
        panic!("expected a space block");
    };
    assert!(space.frame().is_some());
    let geos = space.items();
    assert_eq!(geos.len(), 1);
    let SpaceItem::Geometry(point) = &geos[0] else {
        panic!("expected geometry");
    };
    assert_eq!(point.name().as_deref(), Some("Point"));
    assert_eq!(point.args().len(), 3);
}

#[test]
fn test_chart_args() {
    let source = r#"Chart(data: "p.csv", width: 800, height: 520) {
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let keys: Vec<_> = chart.args().iter().filter_map(|a| a.key()).collect();
    assert_eq!(keys, vec!["data", "width", "height"]);
}

#[test]
fn test_chart_data_stdin() {
    let source = "Chart(data: stdin) {\n}";
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let value = chart.args()[0].value().unwrap();
    assert!(matches!(value, ValueExpr::Stdin(_)));
}

#[test]
fn test_string_literal_value() {
    let source = "Chart(data: \"p.csv\") {\n}";
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let value = chart.args()[0].value().unwrap();
    let ValueExpr::Literal(lit) = value else {
        panic!("expected literal");
    };
    assert_eq!(lit.kind(), Some(LiteralKind::String));
    assert_eq!(lit.text().as_deref(), Some("\"p.csv\""));
}

#[test]
fn test_bare_identifier_value_is_algebra() {
    // `fill: species` is parsed as an algebra value (a column reference).
    let source =
        "Chart(data: \"p.csv\") {\n    Space(a * b) {\n        Point(fill: species)\n    }\n}";
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[0] else {
        panic!()
    };
    let SpaceItem::Geometry(point) = &space.items()[0] else {
        panic!()
    };
    let value = point.args()[0].value().unwrap();
    assert!(matches!(value, ValueExpr::Algebra(_)));
}

#[test]
fn test_derive_declaration() {
    let source = r#"Chart(data: "d.csv") {
    Derive bins = Bin(value, bins: 25)

    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Derive(derive) = &chart.items()[0] else {
        panic!("expected derive");
    };
    assert_eq!(derive.name().as_deref(), Some("bins"));
    let stat = derive.stat().unwrap();
    assert_eq!(stat.name().as_deref(), Some("Bin"));
    assert!(stat.input().is_some()); // `value`
    assert_eq!(stat.args().len(), 1); // `bins: 25`

    // The space binds to the derived table via `data: bins`.
    let ChartItem::Space(space) = &chart.items()[1] else {
        panic!("expected space");
    };
    let data_arg = space
        .args()
        .into_iter()
        .find(|a| a.key().as_deref() == Some("data"));
    assert!(data_arg.is_some());
}

#[test]
fn test_empty_chart() {
    no_errors("Chart(data: \"d.csv\") {\n}");
}

#[test]
fn test_space_local_theme_declaration() {
    let source = r#"Chart(data: "t.csv") {
    Theme(name: "minimal")

    Space(time * value) {
        Theme(name: "void")
        Line(stroke: series)
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    assert!(matches!(chart.items()[0], ChartItem::Theme(_)));
    let ChartItem::Space(space) = &chart.items()[1] else {
        panic!()
    };
    assert!(matches!(space.items()[0], SpaceItem::Theme(_)));
    assert!(matches!(space.items()[1], SpaceItem::Geometry(_)));
}

#[test]
fn test_array_value() {
    let source = r#"Chart(data: "d.csv") {
    Space(gender * height) {
        Violin(fill: gender, quantiles: [0.25, 0.5, 0.75])
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[0] else {
        panic!()
    };
    let SpaceItem::Geometry(violin) = &space.items()[0] else {
        panic!()
    };
    let quantiles = violin.args()[1].value().unwrap();
    let ValueExpr::Array(array) = quantiles else {
        panic!("expected array");
    };
    assert_eq!(array.values().len(), 3);
}

#[test]
fn test_trailing_commas() {
    no_errors("Chart(data: \"d.csv\",) {\n    Space(a * b,) {\n        Point(fill: a,)\n    }\n}");
}

#[test]
fn test_quoted_column_in_algebra_and_property() {
    let source = "Chart(data: \"d.csv\") {\n    Space(`flipper length` * `body mass`) {\n        Point(fill: `species name`)\n    }\n}";
    no_errors(source);
}

#[test]
fn test_dodged_and_nested_algebra() {
    no_errors("Chart(data: \"f.csv\") {\n    Space((quarter / type) * amount) {\n        Bar(fill: type)\n    }\n}");
}

// --- Resilience tests (spec §27.4) ---

fn has_diagnostics(source: &str) -> bool {
    !parse(source).diagnostics().is_empty()
}

fn parses_without_panic(source: &str) -> SyntaxNode {
    parse(source).syntax()
}

#[test]
fn test_missing_space_rhs() {
    // `Space(quarter / )` -> Nest with an error rhs (spec §12.17).
    let source = "Chart(data: \"d.csv\") {\n    Space(quarter / ) {\n        Bar()\n    }\n}";
    assert!(has_diagnostics(source));
    let node = parses_without_panic(source);
    // Tree is still navigable; a space block exists.
    let chart = Root::cast(node).unwrap().chart().unwrap();
    assert!(matches!(chart.items().first(), Some(ChartItem::Space(_))));
}

#[test]
fn test_missing_closing_brace() {
    let source = "Chart(data: \"d.csv\") {\n    Space(a * b) {\n        Point()\n}";
    assert!(has_diagnostics(source));
    parses_without_panic(source);
}

#[test]
fn test_missing_close_paren() {
    let source = "Chart(data: \"d.csv\" {\n}";
    assert!(has_diagnostics(source));
    parses_without_panic(source);
}

#[test]
fn test_unterminated_string_in_chart() {
    // Spec §12.17: a `Chart` with a `data` argument whose value is an
    // unterminated string, recovered without panicking.
    let source = "Chart(data: \"fi";
    let parsed = parse(source);
    assert!(!parsed.diagnostics().is_empty());
    let chart = Root::cast(parsed.syntax()).unwrap().chart();
    assert!(chart.is_some());
}

#[test]
fn test_incomplete_derive_value() {
    // `Bin(value, bins: )` -> derive + stat call + error value (spec §12.17).
    let source = "Chart(data: \"d.csv\") {\n    Derive bins = Bin(value, bins: )\n}";
    assert!(has_diagnostics(source));
    let chart = Root::cast(parse(source).syntax()).unwrap().chart().unwrap();
    let ChartItem::Derive(derive) = &chart.items()[0] else {
        panic!("expected derive");
    };
    assert_eq!(derive.stat().unwrap().name().as_deref(), Some("Bin"));
}

#[test]
fn test_missing_brace_before_following_space() {
    // A missing `}` before a following `Space`: the parser should close the
    // first space and parse the second as a sibling (spec §12.17).
    let source = r#"Chart(data: "d.csv") {
    Space(a * b) {
        Point()

    Space(c * d) {
        Line()
    }
}"#;
    assert!(has_diagnostics(source));
    let chart = Root::cast(parse(source).syntax()).unwrap().chart().unwrap();
    let spaces = chart
        .items()
        .iter()
        .filter(|i| matches!(i, ChartItem::Space(_)))
        .count();
    assert_eq!(spaces, 2, "both spaces should be recovered as siblings");
}

#[test]
fn test_top_level_garbage() {
    assert!(has_diagnostics("@#$ Chart(data: \"d.csv\") {}"));
    parses_without_panic("garbage tokens here");
    parses_without_panic("");
}

#[test]
fn test_misspelled_chart_keyword_still_recovers_body() {
    let source = "Chafrt(data: \"p.csv\") {\n    Space(x * y) {\n        Point()\n    }\n}";
    let parsed = parse(source);
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0001"));
    let chart = Root::cast(parsed.syntax()).unwrap().chart().unwrap();
    assert!(matches!(chart.items()[0], ChartItem::Space(_)));
}

#[test]
fn test_misspelled_space_keyword_still_parses_space() {
    let source = "Chart(data: \"p.csv\") {\n    Sdace(x * y) {\n        Point()\n    }\n}";
    let parsed = parse(source);
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0011"));
    let chart = Root::cast(parsed.syntax()).unwrap().chart().unwrap();
    assert!(matches!(chart.items()[0], ChartItem::Space(_)));
}

#[test]
fn test_invalid_geometry_body() {
    let source =
        "Chart(data: \"d.csv\") {\n    Space(a * b) {\n        12345\n        Point()\n    }\n}";
    assert!(has_diagnostics(source));
    let chart = Root::cast(parse(source).syntax()).unwrap().chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[0] else {
        panic!()
    };
    // The valid Point geometry after the garbage is still recovered.
    assert!(space
        .items()
        .iter()
        .any(|i| matches!(i, SpaceItem::Geometry(_))));
}

#[test]
fn test_let_binding_parses_at_chart_and_space_scope() {
    let source = r##"Chart(data: "p.csv") {
    let primary = "#3366cc"
    Space(flipper_length * body_mass) {
        let dim = 0.4
        Point(fill: primary, alpha: dim)
    }
}"##;
    no_errors(source);
    use algraf_syntax::ast::LetDecl;
    let chart = root(source).chart().unwrap();
    let items = chart.items();
    let ChartItem::Let(binding) = &items[0] else {
        panic!("expected a let binding, got {items:?}");
    };
    assert_eq!(binding.name().as_deref(), Some("primary"));
    let _: &LetDecl = binding;
    let ChartItem::Space(space) = &items[1] else {
        panic!("expected a space block");
    };
    let SpaceItem::Let(space_let) = &space.items()[0] else {
        panic!("expected a space-scope let binding");
    };
    assert_eq!(space_let.name().as_deref(), Some("dim"));
    assert!(
        matches!(space_let.value(), Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number))
    );
}

#[test]
fn test_let_missing_equals_recovers() {
    let parsed = parse("Chart(data: \"p.csv\") {\n  let x 5\n  Space(a) { Point() }\n}");
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0021"));
}

#[test]
fn test_multiple_chart_blocks_parse() {
    let source = "Chart(data: \"a.csv\") {\n    Space(x * y) { Point() }\n}\nChart(data: \"b.csv\") {\n    Space(x * y) { Line() }\n}";
    no_errors(source);
    let charts = Root::cast(parse(source).syntax()).unwrap().charts();
    assert_eq!(charts.len(), 2);
}
