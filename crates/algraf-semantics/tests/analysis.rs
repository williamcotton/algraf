//! Semantic analysis tests (spec §13, §27.5).

use algraf_data::{ColumnDef, DataType};
use algraf_semantics::{analyze_source, FrameIr, GeometryKind, SpaceDataRef};

fn col(name: &str, dtype: DataType) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        dtype,
        nullable: false,
        examples: vec![],
    }
}

/// A schema covering the columns used across these tests.
fn schema() -> Vec<ColumnDef> {
    vec![
        col("flipper_length", DataType::Float),
        col("body_mass", DataType::Float),
        col("species", DataType::String),
        col("quarter", DataType::String),
        col("type", DataType::String),
        col("amount", DataType::Float),
        col("value", DataType::Float),
        col("time", DataType::Temporal),
        col("lower", DataType::Float),
        col("upper", DataType::Float),
        col("group", DataType::String),
    ]
}

fn codes(source: &str) -> Vec<&'static str> {
    analyze_source(source, &schema())
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

fn has(source: &str, code: &str) -> bool {
    codes(source).contains(&code)
}

fn clean(source: &str) {
    let diags = analyze_source(source, &schema()).diagnostics;
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

#[test]
fn test_valid_scatter_is_clean() {
    clean("Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(fill: species, alpha: 0.7, size: 3)\n  }\n}");
}

#[test]
fn test_unknown_column() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * nope) {\n    Point()\n  }\n}",
        "E1101"
    ));
}

#[test]
fn test_quoted_column_resolution() {
    // A quoted column resolves by exact name.
    let schema = vec![
        col("flipper length", DataType::Float),
        col("body mass", DataType::Float),
    ];
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(`flipper length` * `body mass`) {\n    Point()\n  }\n}",
        &schema,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn test_derived_table_resolution() {
    clean("Chart(data: \"d.csv\") {\n  Derive bins = Bin(value, bins: 25)\n  Space(bin_start * count, data: bins) {\n    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)\n  }\n}");
}

#[test]
fn test_unknown_derived_table() {
    assert!(has(
        "Chart(data: \"d.csv\") {\n  Space(a * b, data: missing) {\n    Point()\n  }\n}",
        "E1103"
    ));
}

#[test]
fn test_duplicate_derived_table() {
    assert!(has(
        "Chart(data: \"d.csv\") {\n  Derive bins = Bin(value)\n  Derive bins = Bin(amount)\n  Space(value * amount) { Point() }\n}",
        "E1104"
    ));
}

#[test]
fn test_unknown_geometry_with_suggestion() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Piont()\n  }\n}",
        &schema(),
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == "E1201")
        .expect("E1201");
    assert!(diag.help.as_deref().unwrap().contains("Point"));
}

#[test]
fn test_duplicate_property() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(alpha: 0.5, alpha: 0.7)\n  }\n}",
        "E1203"
    ));
}

#[test]
fn test_unknown_property_colour_is_not_alias() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(colour: species)\n  }\n}",
        &schema(),
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == "E1202")
        .expect("E1202");
    assert!(diag.help.as_deref().unwrap().contains("fill"));
}

#[test]
fn test_property_type_mismatch() {
    // `alpha: "high"` is a string where a number/column is expected.
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(alpha: \"high\")\n  }\n}",
        "E1204"
    ));
}

#[test]
fn test_bare_enum_value_suggests_quoting() {
    let analysis = analyze_source(
        "Chart(data: \"f.csv\") {\n  Space(quarter * amount) {\n    Bar(layout: stack)\n  }\n}",
        &schema(),
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == "E1204")
        .expect("E1204");
    assert!(diag.help.as_deref().unwrap().contains("stack"));
}

#[test]
fn test_unsupported_3d_space() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass * species) {\n    Point()\n  }\n}",
        &schema(),
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == "E1306")
        .expect("E1306");
    assert!(diag.help.as_deref().unwrap().contains("/"));
}

#[test]
fn test_facet_is_allowed() {
    clean("Chart(data: \"p.csv\") {\n  Space((flipper_length * body_mass) / species) {\n    Point()\n  }\n}");
}

#[test]
fn test_facet_requires_categorical_panel_column() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space((flipper_length * body_mass) / amount) {\n    Point()\n  }\n}",
        "E1303"
    ));
}

#[test]
fn test_layout_facet_columns_is_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Layout(facetColumns: 2)\n  Space((flipper_length * body_mass) / species) {\n    Point()\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.layout.facet_columns, Some(2));
}

#[test]
fn test_unparenthesized_blend_rejected() {
    assert!(has(
        "Chart(data: \"i.csv\") {\n  Space(time * lower + upper) {\n    Ribbon(ymin: lower, ymax: upper)\n  }\n}",
        "E1305"
    ));
}

#[test]
fn test_parenthesized_blend_is_clean() {
    clean("Chart(data: \"i.csv\") {\n  Space(time * (lower + upper)) {\n    Ribbon(ymin: lower, ymax: upper, fill: \"steelblue\")\n  }\n}");
}

#[test]
fn test_bar_dodge_hint() {
    // A fill-grouped bar in a plain Cartesian space hints at dodging.
    assert!(has(
        "Chart(data: \"f.csv\") {\n  Space(quarter * amount) {\n    Bar(fill: type)\n  }\n}",
        "H3001"
    ));
}

#[test]
fn test_stacked_bar_has_no_dodge_hint() {
    let cs = codes("Chart(data: \"f.csv\") {\n  Space(quarter * amount) {\n    Bar(fill: type, layout: \"stack\")\n  }\n}");
    assert!(!cs.contains(&"H3001"), "stacked bar should not hint dodge");
    assert!(
        !cs.iter().any(|c| c.starts_with("E")),
        "stacked bar should be valid: {cs:?}"
    );
}

#[test]
fn test_dodged_bar_via_nesting_is_clean() {
    clean("Chart(data: \"f.csv\") {\n  Space((quarter / type) * amount) {\n    Bar(fill: type)\n  }\n}");
}

#[test]
fn test_missing_data_argument() {
    assert!(has(
        "Chart(width: 800) {\n  Space(value * amount) { Point() }\n}",
        "E1001"
    ));
}

#[test]
fn test_missing_required_property() {
    // HLine requires `y`.
    assert!(has(
        "Chart(data: \"t.csv\") {\n  Space(time * value) {\n    HLine(stroke: \"red\")\n  }\n}",
        "E1205"
    ));
}

// --- IR shape ---

#[test]
fn test_ir_is_produced() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\", width: 640, height: 480) {\n  Space(flipper_length * body_mass) {\n    Point(fill: species)\n  }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.width, 640);
    assert_eq!(ir.height, 480);
    assert_eq!(ir.spaces.len(), 1);
    let space = &ir.spaces[0];
    assert_eq!(space.data, SpaceDataRef::Primary);
    assert!(matches!(space.frame, FrameIr::Cartesian(ref axes) if axes.len() == 2));
    assert_eq!(space.geometries.len(), 1);
    assert_eq!(space.geometries[0].kind, GeometryKind::Point);
    assert_eq!(space.geometries[0].mappings[0].aesthetic, "fill");
}

#[test]
fn test_ir_derived_table_schema() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Derive bins = Bin(value, bins: 25)\n  Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let names: Vec<&str> = ir.derived_tables[0]
        .output_schema
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, vec!["bin_start", "bin_end", "bin_center", "count"]);
    assert_eq!(ir.spaces[0].data, SpaceDataRef::Derived("bins".into()));
}
