//! Block grammar parser tests (spec §7, §12.7–12.14, §27.3–27.4).

use algraf_core::Severity;
use algraf_syntax::ast::{ChartItem, LiteralKind, Root, RootItem, SpaceItem, ValueExpr};
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
fn test_space_parses_on_event_emitter_as_call_item() {
    let source = r#"Chart(data: "p.csv") {
    Space(x * y) {
        Point(fill: group)
        On(event: "click", emit: group)
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[0] else {
        panic!("expected a space block");
    };
    let items = space.items();
    assert_eq!(items.len(), 2);
    let SpaceItem::Geometry(on) = &items[1] else {
        panic!("expected call item");
    };
    assert_eq!(on.name().as_deref(), Some("On"));
    assert_eq!(on.args().len(), 2);
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
fn test_chart_data_named_table_reference() {
    let source = "Table main = \"p.csv\"\nChart(data: main) {\n}";
    no_errors(source);
    let root = root(source);
    assert_eq!(root.tables().len(), 1);
    let chart = root.chart().unwrap();
    let value = chart.args()[0].value().unwrap();
    assert!(matches!(value, ValueExpr::Algebra(_)));
}

#[test]
fn test_chart_without_argument_list() {
    let source = "Chart {\n  Table main = \"p.csv\"\n  Space(x * y, data: main) { Point() }\n}";
    no_errors(source);
    let chart = root(source).chart().unwrap();
    assert!(chart.args().is_empty());
    assert!(matches!(chart.items()[0], ChartItem::Table(_)));
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
fn test_sigiled_variable_reference_value() {
    let source = "Chart(data: \"p.csv\") {\n    let primary = \"#3366cc\"\n    Space(a * b) {\n        Point(fill: $primary)\n    }\n}";
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[1] else {
        panic!()
    };
    let SpaceItem::Geometry(point) = &space.items()[0] else {
        panic!()
    };
    let value = point.args()[0].value().unwrap();
    let ValueExpr::Variable(var) = value else {
        panic!("expected a sigiled variable reference");
    };
    assert_eq!(var.name().as_deref(), Some("primary"));
    assert_eq!(
        &source[var.reference_span().start..var.reference_span().end],
        "$primary"
    );
}

#[test]
fn test_malformed_sigiled_variable_reference_recovers() {
    let parsed = parse("Chart(data: \"p.csv\") {\n  Space(a * b) { Point(fill: $) }\n}");
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0010"));
    parses_without_panic("Chart(data: \"p.csv\") {\n  Space(a * b) { Point(fill: $) }\n}");
}

#[test]
fn test_external_placeholder_value_recovers_without_arg_cascade() {
    let source =
        "Chart(data: \"p.csv\") {\n  Space(a * b) { Point(stroke: ${primary}, fill: species) }\n}";
    let parsed = parse(source);
    let codes: Vec<_> = parsed.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["H3006"]);
    assert_eq!(parsed.diagnostics()[0].severity, Severity::Information);

    let chart = root(source).chart().unwrap();
    let ChartItem::Space(space) = &chart.items()[0] else {
        panic!()
    };
    let SpaceItem::Geometry(point) = &space.items()[0] else {
        panic!()
    };
    let value = point.args()[0].value().unwrap();
    assert!(matches!(value, ValueExpr::Error(_)));
}

#[test]
fn test_spaced_sigiled_variable_reference_is_diagnostic() {
    let parsed = parse("Chart(data: \"p.csv\") {\n  Space(a * b) { Point(fill: $ primary) }\n}");
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0010"));
}

#[test]
fn test_glyph_decl_contains_child_space_and_key() {
    let source = r#"Chart(data: "p.csv") {
    Table mix = "mix.csv"
    Glyph pie(data: mix, key: [id], size: 32) {
        Space(value, coords: "polar", theta: "y") {
            Bar(fill: category, layout: "fill")
        }
    }
    Space(x * y) {
        pie(clip: "circle")
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Glyph(glyph) = &chart.items()[1] else {
        panic!("expected glyph declaration");
    };
    assert_eq!(glyph.name().as_deref(), Some("pie"));
    assert_eq!(glyph.args().len(), 3);
    assert!(matches!(
        glyph.args()[1].value().unwrap(),
        ValueExpr::Array(_)
    ));
    assert_eq!(glyph.items().len(), 1);
}

#[test]
fn test_malformed_glyph_body_recovers_following_item() {
    let source = r#"Chart(data: "p.csv") {
    Table child = "child.csv"
    Glyph spark(data: child, key: [id], size: 32) {
        12345
        Space(value) { Point() }
    }
    Space(x * y) {
        Text(label: id)
    }
}"#;
    assert!(has_diagnostics(source));
    let chart = root(source).chart().unwrap();
    assert!(chart
        .items()
        .iter()
        .any(|item| matches!(item, ChartItem::Glyph(_))));
    let ChartItem::Space(space) = &chart.items()[2] else {
        panic!("expected space");
    };
    assert!(space.items().iter().any(
        |item| matches!(item, SpaceItem::Geometry(geo) if geo.name().as_deref() == Some("Text"))
    ));
}

#[test]
fn test_missing_glyph_closing_brace_recovers_without_panic() {
    let source = r#"Chart(data: "p.csv") {
    Table child = "child.csv"
    Glyph spark(data: child, key: [id]) {
        Space(value) { Point() }
}"#;
    assert!(has_diagnostics(source));
    parses_without_panic(source);
}

#[test]
fn test_deeply_nested_glyph_decls_are_bounded_and_navigable() {
    let source = r#"Chart(data: "p.csv") {
    Table child = "child.csv"
    Glyph outer_glyph(data: child, key: [id], size: 40) {
        Space(value) {
            inner_glyph(size: 20)
        }
    }
    Glyph inner_glyph(data: child, key: [id], size: 20) {
        Space(value) { Point() }
    }
    Space(x * y) {
        outer_glyph(size: 10)
    }
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    assert!(matches!(chart.items().get(1), Some(ChartItem::Glyph(_))));
    assert!(matches!(chart.items().get(2), Some(ChartItem::Glyph(_))));
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
    assert_eq!(derive.source_name(), None);
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
fn test_derive_from_declaration() {
    let source = r#"Chart(data: "d.csv") {
    Derive bins = Bin(value, bins: 25)
    Derive trend from bins = Smooth(bin_center, count)
}"#;
    no_errors(source);
    let chart = root(source).chart().unwrap();
    let ChartItem::Derive(derive) = &chart.items()[1] else {
        panic!("expected derive");
    };
    assert_eq!(derive.name().as_deref(), Some("trend"));
    assert_eq!(derive.source_name().as_deref(), Some("bins"));
    assert!(derive.source_name_span().is_some());
    assert_eq!(derive.stat().unwrap().name().as_deref(), Some("Smooth"));
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
        Point(fill: $primary, alpha: $dim)
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
fn test_document_scope_let_parses_around_tables_and_charts() {
    let source = r##"Algraf(version: "0.20")
let ink = "#333333"
Table main = "p.csv"
Chart(data: main) {
    Space(x * y) { Point(fill: $ink) }
}
let faint = "#dddddd"
Chart(data: main) {
    Space(x * y) { Line(stroke: $faint) }
}"##;
    no_errors(source);
    let root = root(source);
    assert_eq!(root.lets().len(), 2);
    assert_eq!(root.tables().len(), 1);
    assert_eq!(root.charts().len(), 2);
    let items = root.items();
    assert!(matches!(items[0], RootItem::Let(_)));
    assert!(matches!(items[1], RootItem::Table(_)));
    assert!(matches!(items[2], RootItem::Chart(_)));
    assert!(matches!(items[3], RootItem::Let(_)));
    assert!(matches!(items[4], RootItem::Chart(_)));
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

#[test]
fn test_source_header_parses_before_chart() {
    let source =
        "Algraf(version: \"0.20\", features: [\"experimental\"])\n\nChart(data: \"a.csv\") {}";
    no_errors(source);
    let root = root(source);
    let header = root.source_header().expect("source header");
    assert_eq!(header.args().len(), 2);
    assert_eq!(root.charts().len(), 1);
}

#[test]
fn test_source_header_diagnostics() {
    let parsed = parse(
        "Algraf(version: \"9.0\", features: [\"sql\", \"sql\", \"wat\"])\nChart(data: \"a.csv\") {}",
    );
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0023"));
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0024"));
}

#[test]
fn test_sqlite_requires_sql_feature_gate() {
    let parsed = parse(
        "Algraf(version: \"0.21\")\nChart(data: Sqlite(\"sales.db\", \"SELECT region FROM sales ORDER BY region\")) {}",
    );
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0025"));

    let source = "Algraf(version: \"0.21\", features: [\"sql\"])\nChart(data: Sqlite(\"sales.db\", \"SELECT region FROM sales ORDER BY region\")) {}";
    no_errors(source);
}

// --- v0.6.0: Table declarations and map literals (spec §7.4, §7.8) ---

use algraf_syntax::ast::{MapValue, TableDecl};

#[test]
fn test_table_declaration_parses() {
    let source =
        "Chart(data: \"t.csv\") {\n  Table cities = \"c.csv\"\n  Space(x * y, data: cities) { Point() }\n}";
    no_errors(source);
    let chart = root(source).chart().expect("chart");
    let ChartItem::Table(decl) = chart
        .items()
        .into_iter()
        .find(|i| matches!(i, ChartItem::Table(_)))
        .expect("table decl")
    else {
        unreachable!()
    };
    let decl: TableDecl = decl;
    assert_eq!(decl.name().as_deref(), Some("cities"));
    assert!(matches!(decl.source(), Some(ValueExpr::Literal(_))));
}

#[test]
fn test_map_literal_parses_distinct_from_array() {
    let source = "Chart(data: \"t.csv\") {\n  Scale(stroke: dir, range: [\"A\" => \"burlywood\", \"R\" => \"black\"])\n  Space(x * y) { Point(stroke: dir) }\n}";
    no_errors(source);
    // The map value node has two entries with key/value pairs.
    let syntax = parse(source).syntax();
    let map = syntax
        .descendants()
        .find_map(MapValue::cast)
        .expect("map value");
    let entries = map.entries();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].key().is_some());
    assert!(entries[0].value().is_some());
}

#[test]
fn test_map_entry_missing_arrow_recovers() {
    // A bracket containing a `=>` is a map; a later entry without `=>` is E0021.
    let parsed = parse(
        "Chart(data: \"t.csv\") {\n  Scale(stroke: d, range: [\"A\" => \"x\", \"B\"])\n  Space(a) { Point() }\n}",
    );
    assert!(parsed.diagnostics().iter().any(|d| d.code == "E0021"));
}
