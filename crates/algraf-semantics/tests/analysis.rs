//! Semantic analysis tests (spec §13, §27.5).

use algraf_data::{ColumnDef, DataType};
use algraf_semantics::{
    analyze_source, AxisSelectorIr, BinClosedIr, FrameIr, GeometryKind, PropertyKey, ScaleTargetIr,
    ScaleTypeIr, SettingValue, SpaceDataRef, StatKind, StatOptionsIr,
};

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
        col("geom", DataType::Geometry),
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

/// Analyzer resilience: deeply nested algebra must analyze without panicking,
/// recursion overflow, or hangs (spec §12.1, §13.17, §27.4). The analyzer walks
/// the frame tree recursively, so it shares the parser's nesting risk.
#[test]
fn deeply_nested_algebra_analyzes_without_panic() {
    let depth = 400;
    let src = format!(
        "Chart(data: \"p.csv\") {{\n  Space({}flipper_length{}) {{ Point() }}\n}}",
        "(".repeat(depth),
        ")".repeat(depth),
    );
    // Must terminate and produce an analysis result (with or without diagnostics)
    // rather than crashing.
    let _ = analyze_source(&src, &schema());
}

#[test]
fn deeply_nested_cross_chain_analyzes_without_panic() {
    let depth = 400;
    let chain = std::iter::repeat("flipper_length")
        .take(depth)
        .collect::<Vec<_>>()
        .join(" * ");
    let src = format!("Chart(data: \"p.csv\") {{\n  Space({chain}) {{ Point() }}\n}}");
    let _ = analyze_source(&src, &schema());
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
fn test_unknown_column_span_excludes_leading_whitespace() {
    let source =
        "Chart(data: \"p.csv\") {\n  Space(\n    regin * amount\n  ) {\n    Point()\n  }\n}";
    let analysis = analyze_source(
        source,
        &[
            col("region", DataType::String),
            col("amount", DataType::Float),
        ],
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|diag| diag.code == "E1101")
        .expect("expected unknown-column diagnostic");
    let start = source.find("regin").unwrap();
    assert_eq!(diag.span.start, start);
    assert_eq!(diag.span.end, start + "regin".len());
    assert_eq!(diag.help.as_deref(), Some("did you mean `region`?"));
}

#[test]
fn test_misspelled_chart_and_space_still_report_column_errors() {
    let source = "Chafrt(data: \"regional_sales.csv\") {\n    Sdace((time * sales) / regon) {\n        Line(stroke: product)\n    }\n}";
    let schema = [
        col("time", DataType::Temporal),
        col("sales", DataType::Float),
        col("region", DataType::String),
        col("product", DataType::String),
    ];
    let diagnostics = analyze_source(source, &schema).diagnostics;
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(codes.contains(&"E0001"), "{diagnostics:?}");
    assert!(codes.contains(&"E0011"), "{diagnostics:?}");
    assert!(codes.contains(&"E1101"), "{diagnostics:?}");

    let column = diagnostics
        .iter()
        .find(|diag| diag.code == "E1101")
        .expect("expected unknown column");
    let start = source.find("regon").unwrap();
    assert_eq!(column.span.start, start);
    assert_eq!(column.span.end, start + "regon".len());
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
fn test_chained_derived_table_resolution() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Derive bins = Bin(value, bins: 4)\n  Derive trend = Smooth(bin_center, count, method: \"lm\")\n  Space(x * y, data: trend) { Line() }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 2);
    assert_eq!(ir.derived_tables[1].name, "trend");
    assert_eq!(
        ir.derived_tables[1].data,
        SpaceDataRef::Derived("bins".into())
    );
    assert_eq!(ir.derived_tables[1].stat.kind, StatKind::Smooth);
}

#[test]
fn test_derived_cycle_is_diagnostic() {
    assert!(has(
        "Chart(data: \"d.csv\") {\n  Derive a = Bin(bin_center)\n  Derive b = Bin(count)\n  Space(value * amount) { Point() }\n}",
        "E1501"
    ));
}

#[test]
fn test_bin_rejects_bins_and_bin_width_together() {
    assert!(has(
        "Chart(data: \"d.csv\") {\n  Derive bins = Bin(value, bins: 25, binWidth: 1)\n  Space(bin_start * count, data: bins) {\n    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)\n  }\n}",
        "E1404"
    ));
}

#[test]
fn test_bin_closed_requires_string_enum() {
    assert!(has(
        "Chart(data: \"d.csv\") {\n  Derive bins = Bin(value, closed: left)\n  Space(bin_start * count, data: bins) {\n    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)\n  }\n}",
        "E1404"
    ));
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
fn test_chart_labels_and_guide_legend_are_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\", title: \"Sales\", subtitle: \"By region\", caption: \"Source: test\") {\n  Guide(legend: false)\n  Space(flipper_length * body_mass) {\n    Point(fill: species)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.title.as_deref(), Some("Sales"));
    assert_eq!(ir.subtitle.as_deref(), Some("By region"));
    assert_eq!(ir.caption.as_deref(), Some("Source: test"));
    assert!(!ir.guides.legend);
}

#[test]
fn test_scale_declaration_is_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(axis: x, type: \"log10\", domain: [1, 100], reverse: true)\n  Scale(fill: species, palette: \"accent\")\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.scales.len(), 2);
    assert!(matches!(
        ir.scales[0].target,
        ScaleTargetIr::Axis(AxisSelectorIr::X)
    ));
    assert_eq!(ir.scales[0].scale_type, Some(ScaleTypeIr::Log10));
    assert_eq!(ir.scales[0].domain, Some([Some(1.0), Some(100.0)]));
    assert_eq!(ir.scales[0].reverse, Some(true));
    assert!(matches!(
        &ir.scales[1].target,
        ScaleTargetIr::Aesthetic {
            aesthetic,
            column: Some(column)
        } if aesthetic == "fill" && column.name == "species"
    ));
    assert_eq!(ir.scales[1].palette.as_deref(), Some("accent"));
}

#[test]
fn test_scale_integer_is_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(axis: y, integer: true)\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.scales[0].integer, Some(true));
}

#[test]
fn test_scale_integer_rejects_non_boolean_and_aesthetic_target() {
    let bad_value = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(axis: y, integer: 1)\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    assert!(bad_value
        .diagnostics
        .iter()
        .any(|d| d.code == "E1204" && d.message.contains("integer")));

    let wrong_target = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(fill: species, integer: true)\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        &schema(),
    );
    assert!(wrong_target
        .diagnostics
        .iter()
        .any(|d| d.code == "E1204" && d.message.contains("integer")));
}

#[test]
fn test_scale_label_is_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(fill: species, label: \"Penguin Species\")\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.scales[0].label.as_deref(), Some("Penguin Species"));
}

#[test]
fn test_scale_label_must_be_string() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Scale(fill: species, label: 3)\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        "E1204"
    ));
}

#[test]
fn test_scale_gradient_is_recorded_for_continuous_fill() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Scale(fill: value, gradient: [\"#3366cc\", \"#cc3333\"], label: \"Value\")\n  Space(flipper_length * body_mass) { Point(fill: value) }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(
        ir.scales[0].gradient.as_ref().unwrap(),
        &vec!["#3366cc".to_string(), "#cc3333".to_string()]
    );
    assert_eq!(ir.scales[0].label.as_deref(), Some("Value"));
}

#[test]
fn test_scale_gradient_rejects_bad_arrays_and_categorical_columns() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Scale(fill: value, gradient: [\"#3366cc\", \"not-a-color\"])\n  Space(flipper_length * body_mass) { Point(fill: value) }\n}",
        "E1601"
    ));
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Scale(fill: species, gradient: [\"#3366cc\", \"#cc3333\"])\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        "E1602"
    ));
}

#[test]
fn test_theme_name_is_validated() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Theme(name: \"neon\")\n  Space(flipper_length * body_mass) { Point() }\n}",
        "E1204"
    ));
    clean("Chart(data: \"p.csv\") {\n  Theme(name: \"light\")\n  Space(flipper_length * body_mass) { Point() }\n}");
}

#[test]
fn test_empty_space_warns() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {}\n}",
        "W2001"
    ));
}

#[test]
fn test_temporal_nesting_warns_about_cardinality() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space((time / group) * value) {\n    Line()\n  }\n}",
        "W2008"
    ));
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
fn test_smooth_loess_is_deferred_in_version_0_1() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Smooth(method: \"loess\")\n  }\n}",
        "E1204"
    ));
}

#[test]
fn test_violin_is_registered() {
    clean("Chart(data: \"p.csv\") {\n  Space(species * body_mass) {\n    Violin(quantiles: [0.25, 0.5, 0.75], fill: species)\n  }\n}");
}

#[test]
fn test_line_and_smooth_accept_group_aesthetic() {
    clean("Chart(data: \"p.csv\") {\n  Space(time * value) {\n    Line(group: group, stroke: \"#888888\")\n    Smooth(group: group, stroke: \"#444444\")\n  }\n}");
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

#[test]
fn test_direct_histogram_desugars_to_bin_and_rect() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Space(value) {\n    Histogram(bins: 4, fill: \"steelblue\", alpha: 0.7)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let derived = &ir.derived_tables[0];
    assert!(derived.name.starts_with("__histogram_"));
    assert_eq!(derived.stat.kind, StatKind::Bin);
    // The typed stat options carry `bins: 4` directly (spec §13.4).
    assert!(matches!(
        derived.stat.options,
        StatOptionsIr::Bin {
            bins: Some(b),
            ..
        } if b == 4.0
    ));

    assert_eq!(ir.spaces.len(), 1);
    assert_eq!(
        ir.spaces[0].data,
        SpaceDataRef::Derived(derived.name.clone())
    );
    assert!(matches!(ir.spaces[0].frame, FrameIr::Cartesian(ref axes) if axes.len() == 2));
    assert_eq!(ir.spaces[0].geometries.len(), 1);
    assert_eq!(ir.spaces[0].geometries[0].kind, GeometryKind::Rect);
}

#[test]
fn test_direct_histogram_name_avoids_user_derived_names() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Space(value) {\n    Histogram()\n  }\n  Derive __histogram_0 = Bin(value)\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let synthetic = ir
        .derived_tables
        .iter()
        .find(|table| table.name.starts_with("__histogram_") && table.name != "__histogram_0")
        .expect("synthetic histogram table");
    assert_ne!(synthetic.name, "__histogram_0");
    assert!(ir
        .derived_tables
        .iter()
        .any(|table| table.name == "__histogram_0"));
}

#[test]
fn test_histogram_temporal_input_desugars_to_temporal_bins() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Space(time) {\n    Histogram()\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(
        ir.derived_tables[0].output_schema[0].dtype,
        DataType::Temporal
    );
    assert!(matches!(ir.spaces[0].frame, FrameIr::Cartesian(ref axes)
        if matches!(&axes[0], FrameIr::Vector(column) if column.dtype == DataType::Temporal)));
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
    assert_eq!(space.geometries[0].mappings[0].aesthetic, PropertyKey::Fill);
}

#[test]
fn test_ir_records_chart_margins() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\", marginRight: 150, marginTop: 12) {\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.margin_right, Some(150));
    assert_eq!(ir.margin_top, Some(12));
    assert_eq!(ir.margin_left, None);
    assert_eq!(ir.margin_bottom, None);
}

#[test]
fn test_text_dy_column_and_declutter_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(value * amount) {\n    Text(label: species, dy: amount, declutter: true)\n  }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    let text = &ir.spaces[0].geometries[0];
    assert_eq!(text.kind, GeometryKind::Text);
    // `dy: amount` is a column mapping, not a setting.
    assert!(text
        .mappings
        .iter()
        .any(|m| m.aesthetic == PropertyKey::Dy && m.column.name == "amount"));
    // `declutter: true` is a boolean setting.
    assert!(text
        .settings
        .iter()
        .any(|s| s.name == PropertyKey::Declutter && matches!(s.value, SettingValue::Bool(true))));
}

#[test]
fn test_mapping_and_setting_preserve_authored_spans() {
    // Each mapping and setting carries the byte span of the user-authored
    // argument that produced it (spec §13.6).
    let source =
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(fill: species, alpha: 0.7)\n  }\n}";
    let analysis = analyze_source(source, &schema());
    let point = &analysis.ir.expect("ir").spaces[0].geometries[0];
    let fill = point
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Fill)
        .expect("fill mapping");
    assert_eq!(&source[fill.span.start..fill.span.end], "fill: species");
    let alpha = point
        .settings
        .iter()
        .find(|s| s.name == PropertyKey::Alpha)
        .expect("alpha setting");
    assert_eq!(&source[alpha.span.start..alpha.span.end], "alpha: 0.7");
}

#[test]
fn test_duplicate_chart_margin_is_reported() {
    assert!(has(
        "Chart(data: \"p.csv\", marginRight: 10, marginRight: 20) {\n  Space(value) { Point() }\n}",
        "E1002",
    ));
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
    assert_eq!(
        names,
        vec!["bin_start", "bin_end", "bin_center", "count", "density"]
    );
    assert_eq!(ir.spaces[0].data, SpaceDataRef::Derived("bins".into()));
}

#[test]
fn test_explicit_bin_derive_carries_typed_options() {
    // `Bin` settings are typed at the semantic/render boundary (spec §13.4):
    // `bins`/`binWidth`/`boundary` are `Option<f64>` and `closed` is an enum.
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Derive b = Bin(value, binWidth: 2.5, boundary: 0, closed: \"right\")\n  Space(bin_start * count, data: b) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    match &ir.derived_tables[0].stat.options {
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
        } => {
            assert_eq!(*bins, None);
            assert_eq!(*bin_width, Some(2.5));
            assert_eq!(*boundary, Some(0.0));
            assert_eq!(*closed, BinClosedIr::Right);
        }
        other => panic!("expected Bin options, got {other:?}"),
    }
}

#[test]
fn test_smooth_derive_defaults_to_lm_method() {
    let analysis = analyze_source(
        "Chart(data: \"d.csv\") {\n  Derive fit = Smooth(value, amount)\n  Space(x * y, data: fit) { Line() }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    assert!(matches!(
        ir.derived_tables[0].stat.options,
        StatOptionsIr::Smooth { .. }
    ));
}

// --- Count stat ---

#[test]
fn test_bar_count_stat_desugars() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(species) {\n    Bar(stat: \"count\", fill: species)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let derived = &ir.derived_tables[0];
    assert!(derived.name.starts_with("__count_"));
    assert_eq!(derived.stat.kind, StatKind::Count);
    let names: Vec<&str> = derived
        .output_schema
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, vec!["species", "count"]);
    assert_eq!(ir.spaces.len(), 1);
    assert_eq!(
        ir.spaces[0].data,
        SpaceDataRef::Derived(derived.name.clone())
    );
    assert!(matches!(ir.spaces[0].frame, FrameIr::Cartesian(ref axes) if axes.len() == 2));
    assert_eq!(ir.spaces[0].geometries[0].kind, GeometryKind::Bar);
}

// --- Density stat ---

#[test]
fn test_density_desugars_to_kde_table_and_area() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(value) {\n    Density(fill: \"#4c78a8\")\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let derived = &ir.derived_tables[0];
    assert!(derived.name.starts_with("__density_"));
    assert_eq!(derived.stat.kind, StatKind::Density);
    let names: Vec<&str> = derived
        .output_schema
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, vec!["density_x", "density"]);
    assert_eq!(ir.spaces.len(), 1);
    assert_eq!(
        ir.spaces[0].data,
        SpaceDataRef::Derived(derived.name.clone())
    );
    assert!(matches!(ir.spaces[0].frame, FrameIr::Cartesian(ref axes) if axes.len() == 2));
    assert_eq!(ir.spaces[0].geometries[0].kind, GeometryKind::Area);
}

#[test]
fn test_density_requires_numeric_column() {
    // A categorical column cannot be density-estimated.
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(species) {\n    Density()\n  }\n}",
        "E1404"
    ));
}

#[test]
fn test_density_rejects_non_vector_space() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Density()\n  }\n}",
        "E1302"
    ));
}

#[test]
fn test_freqpoly_and_2d_binning_geometries_are_registered() {
    clean("Chart(data: \"p.csv\") {\n  Space(value) { FreqPoly(bins: 8, stroke: \"steelblue\") }\n  Space(flipper_length * body_mass) { Bin2D(bins: 6) HexBin(bins: 6) }\n}");
}

#[test]
fn test_freqpoly_desugars_to_bin_table_and_line() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(value) {\n    FreqPoly(bins: 8, stroke: \"steelblue\")\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let derived = &ir.derived_tables[0];
    assert!(derived.name.starts_with("__freqpoly_"));
    assert_eq!(derived.stat.kind, StatKind::Bin);
    let names: Vec<&str> = derived
        .output_schema
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["bin_start", "bin_end", "bin_center", "count", "density"]
    );
    assert_eq!(ir.spaces.len(), 1);
    assert_eq!(
        ir.spaces[0].data,
        SpaceDataRef::Derived(derived.name.clone())
    );
    // FreqPoly draws a line over bin_center * count.
    assert!(matches!(ir.spaces[0].frame, FrameIr::Cartesian(ref axes)
        if matches!(&axes[0], FrameIr::Vector(c) if c.name == "bin_center")));
    assert_eq!(ir.spaces[0].geometries.len(), 1);
    assert_eq!(ir.spaces[0].geometries[0].kind, GeometryKind::Line);
}

#[test]
fn test_bin2d_desugars_to_bin2d_table_and_rect_with_fill() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Bin2D(bins: 6)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.derived_tables.len(), 1);
    let derived = &ir.derived_tables[0];
    assert!(derived.name.starts_with("__bin2d_"));
    assert_eq!(derived.stat.kind, StatKind::Bin2D);
    assert_eq!(ir.spaces.len(), 1);
    let rect = &ir.spaces[0].geometries[0];
    assert_eq!(rect.kind, GeometryKind::Rect);
    // With no explicit fill, count drives the fill mapping.
    assert!(rect
        .mappings
        .iter()
        .any(|m| m.aesthetic == PropertyKey::Fill && m.column.name == "count"));
}

#[test]
fn test_lowered_nodes_carry_source_call_span() {
    // The synthetic derived table and lowered space should point back at the
    // original geometry call so diagnostics stay precise after lowering.
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(value) {\n    Histogram(bins: 4)\n  }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    let call_span = ir.spaces[0].geometries[0].span;
    assert_eq!(ir.derived_tables[0].span, call_span);
    assert_eq!(ir.derived_tables[0].stat.span, call_span);
}

#[test]
fn test_density_bandwidth_must_be_positive() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(value) {\n    Density(bandwidth: 0)\n  }\n}",
        "E1404"
    ));
}

// --- Space-local theme ---

#[test]
fn test_space_local_theme_is_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\")\n  Space(flipper_length * body_mass) {\n    Theme(name: \"void\")\n    Point()\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.theme.as_ref().unwrap().base.as_deref(), Some("minimal"));
    assert_eq!(
        ir.spaces[0].theme.as_ref().unwrap().base.as_deref(),
        Some("void")
    );
}

// --- Guide axis label override ---

#[test]
fn test_guide_axis_label_overrides_are_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Guide(axis: x, label: \"Flipper\")\n  Guide(axis: y, label: \"Mass\")\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.guides.x_label.as_deref(), Some("Flipper"));
    assert_eq!(ir.guides.y_label.as_deref(), Some("Mass"));
}

#[test]
fn test_guide_fill_null_suppresses_legend() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Guide(fill: null)\n  Space(flipper_length * body_mass) { Point(fill: species) }\n}",
        &schema(),
    );
    let ir = analysis.ir.expect("ir");
    assert!(!ir.guides.fill_legend);
}

#[test]
fn test_guide_stroke_and_grid_controls_are_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Guide(stroke: null)\n  Guide(grid: false)\n  Space(flipper_length * body_mass) { Point(stroke: species) }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert!(!ir.guides.stroke_legend);
    assert!(!ir.guides.grid);
}

#[test]
fn test_space_local_scale_and_guide_are_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Scale(axis: y, reverse: true)\n    Guide(axis: y, label: \"Mass\")\n    Point()\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.spaces[0].scales.len(), 1);
    assert_eq!(ir.spaces[0].scales[0].reverse, Some(true));
    assert_eq!(ir.spaces[0].guides.y_label.as_deref(), Some("Mass"));
}

// --- Area, Text, Segment ---

#[test]
fn test_area_geometry_is_registered() {
    clean("Chart(data: \"t.csv\") {\n  Space(time * value) {\n    Area(baseline: 0, fill: \"steelblue\", alpha: 0.4)\n  }\n}");
}

#[test]
fn test_text_geometry_is_registered() {
    clean("Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Text(label: species)\n  }\n}");
}

#[test]
fn test_text_missing_label_is_rejected() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Text()\n  }\n}",
        "E1205"
    ));
}

#[test]
fn test_segment_geometry_is_registered() {
    clean("Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Segment(x: 160, y: 55, xend: 185, yend: 85)\n  }\n}");
}

// --- Diagnostic hints ---

#[test]
fn test_bare_color_name_emits_h3002_hint() {
    // `fill: red` is a bare identifier where a column or color literal is
    // expected. Since `red` is a CSS color name and no such column exists,
    // emit H3002 suggesting quotes.
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    Point(fill: red)\n  }\n}",
        &schema(),
    );
    let diag = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == "H3002")
        .expect("H3002");
    assert!(diag.help.as_deref().unwrap().contains("\"red\""));
}

// --- Let bindings (spec §7.10, §9.6) ---

#[test]
fn test_let_constant_resolves_in_property_value() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  let primary = \"#3366cc\"\n  let dim = 0.4\n  Space(flipper_length * body_mass) {\n    Point(fill: primary, alpha: dim)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let geo = &analysis.ir.expect("ir").spaces[0].geometries[0];
    assert!(geo.settings.iter().any(|s| s.name == PropertyKey::Fill
        && matches!(&s.value, SettingValue::String(v) if v == "#3366cc")));
    assert!(geo
        .settings
        .iter()
        .any(|s| s.name == PropertyKey::Alpha
            && matches!(s.value, SettingValue::Number(n) if n == 0.4)));
}

#[test]
fn test_let_non_constant_value_is_rejected() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  let bad = flipper_length\n  Space(value) { Point() }\n}",
        "E1701",
    ));
}

#[test]
fn test_duplicate_let_binding_is_reported() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  let c = \"#111\"\n  let c = \"#222\"\n  Space(value) { Point() }\n}",
        "E1702",
    ));
}

#[test]
fn test_let_type_mismatch_at_use_site() {
    // A string variable used where a number is expected is an E1204 type error.
    assert!(has(
        "Chart(data: \"p.csv\") {\n  let label = \"x\"\n  Space(flipper_length * body_mass) {\n    Point(alpha: label)\n  }\n}",
        "E1204",
    ));
}

#[test]
fn test_space_let_shadows_chart_let() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  let c = \"#111111\"\n  Space(flipper_length * body_mass) {\n    let c = \"#222222\"\n    Point(fill: c)\n  }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let geo = &analysis.ir.expect("ir").spaces[0].geometries[0];
    assert!(geo.settings.iter().any(|s| s.name == PropertyKey::Fill
        && matches!(&s.value, SettingValue::String(v) if v == "#222222")));
}

#[test]
fn test_space_let_does_not_leak_to_sibling_space() {
    // `local` is bound in the first space only; the second space sees it as an
    // unknown column, not a variable.
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(flipper_length * body_mass) {\n    let local = \"#333\"\n    Point(fill: local)\n  }\n  Space(flipper_length * body_mass) {\n    Point(fill: local)\n  }\n}",
        "E1101",
    ));
}

#[test]
fn test_quoted_identifier_is_not_a_variable() {
    // A backtick identifier always resolves as a column, never a variable.
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  let species = \"#333\"\n  Space(flipper_length * body_mass) {\n    Point(fill: `species`)\n  }\n}",
        &schema(),
    );
    let geo = &analysis.ir.expect("ir").spaces[0].geometries[0];
    assert!(geo
        .mappings
        .iter()
        .any(|m| m.aesthetic == PropertyKey::Fill && m.column.name == "species"));
}

// --- Custom theme objects (spec §20.8) ---

#[test]
fn test_theme_overrides_are_recorded() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\", axisText: Text(size: 12, fill: \"#333333\"), gridMajor: Line(stroke: \"#dddddd\", strokeWidth: 1), plotBackground: \"#fafafa\")\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let theme = analysis.ir.expect("ir").theme.expect("theme");
    assert_eq!(theme.base.as_deref(), Some("minimal"));
    assert_eq!(theme.overrides.font_size, Some(12.0));
    assert_eq!(theme.overrides.text_color.as_deref(), Some("#333333"));
    assert_eq!(theme.overrides.grid_major_color.as_deref(), Some("#dddddd"));
    assert_eq!(theme.overrides.grid_major_width, Some(1.0));
    assert_eq!(theme.overrides.plot_background.as_deref(), Some("#fafafa"));
}

#[test]
fn test_theme_unknown_property_is_reported() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Theme(axisColour: \"#333\")\n  Space(value) { Point() }\n}",
        "E1704",
    ));
}

#[test]
fn test_theme_property_type_mismatch_is_reported() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Theme(fontSize: \"big\")\n  Space(value) { Point() }\n}",
        "E1705",
    ));
}

#[test]
fn test_theme_grouped_override_wrong_shape_is_reported() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Theme(gridMajor: 5)\n  Space(value) { Point() }\n}",
        "E1705",
    ));
}

#[test]
fn test_theme_override_composes_with_let() {
    let analysis = analyze_source(
        "Chart(data: \"p.csv\") {\n  let ink = \"#101010\"\n  Theme(textColor: ink)\n  Space(flipper_length * body_mass) { Point() }\n}",
        &schema(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let theme = analysis.ir.expect("ir").theme.expect("theme");
    assert_eq!(theme.overrides.text_color.as_deref(), Some("#101010"));
}

// --- v0.6.0: named tables, map scales, range/null bounds (spec §10.x, §16) ---

use algraf_semantics::{analyze_with_tables, ScaleIr};
use algraf_syntax::parse as parse_src;
use std::collections::HashMap;

fn analyze_tables(
    source: &str,
    primary: &[ColumnDef],
    tables: &[(&str, Vec<ColumnDef>)],
) -> algraf_semantics::Analysis {
    let parsed = parse_src(source);
    let map: HashMap<String, Vec<ColumnDef>> = tables
        .iter()
        .map(|(n, s)| (n.to_string(), s.clone()))
        .collect();
    let mut analysis = analyze_with_tables(&parsed.syntax(), primary, &map);
    let mut diags = parsed.into_diagnostics();
    diags.append(&mut analysis.diagnostics);
    algraf_semantics::Analysis {
        ir: analysis.ir,
        diagnostics: diags,
    }
}

fn first_scale(ir: &algraf_semantics::ChartIr) -> &ScaleIr {
    ir.scales.first().expect("scale")
}

#[test]
fn test_named_table_resolves_and_binds() {
    let primary = vec![col("long", DataType::Float), col("lat", DataType::Float)];
    let cities = vec![
        col("long", DataType::Float),
        col("lat", DataType::Float),
        col("city", DataType::String),
    ];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Table cities = \"c.csv\"\n  Space(long * lat, data: cities) { Text(label: city, size: 6) }\n}",
        &primary,
        &[("cities", cities)],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.tables.len(), 1);
    assert_eq!(ir.tables[0].name, "cities");
    assert_eq!(ir.tables[0].path, "c.csv");
    assert!(ir
        .spaces
        .iter()
        .any(|s| s.data == SpaceDataRef::Table("cities".into())));
}

#[test]
fn test_duplicate_table_name_e1105() {
    let primary = vec![col("x", DataType::Float)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Table a = \"1.csv\"\n  Table a = \"2.csv\"\n  Space(x) { Point() }\n}",
        &primary,
        &[("a", vec![col("x", DataType::Float)])],
    );
    assert!(analysis.diagnostics.iter().any(|d| d.code == "E1105"));
}

#[test]
fn test_table_conflicts_with_derived_e1108() {
    let primary = vec![col("value", DataType::Float)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Derive a = Bin(value)\n  Table a = \"a.csv\"\n  Space(value) { Point() }\n}",
        &primary,
        &[("a", vec![col("value", DataType::Float)])],
    );
    assert!(analysis.diagnostics.iter().any(|d| d.code == "E1108"));
}

#[test]
fn test_strokewidth_scale_requires_numeric_e1607() {
    let primary = vec![col("x", DataType::Float), col("name", DataType::String)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Scale(strokeWidth: name, range: [0, 10])\n  Space(x) { Point() }\n}",
        &primary,
        &[],
    );
    assert!(analysis.diagnostics.iter().any(|d| d.code == "E1607"));
}

#[test]
fn test_manual_color_map_and_labels() {
    let primary = vec![col("long", DataType::Float), col("dir", DataType::String)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Scale(stroke: dir, range: [\"A\" => \"burlywood\", \"R\" => \"black\"], labels: [\"A\" => \"Advance\", \"R\" => \"Retreat\"], label: \"Direction\")\n  Space(long) { Point(stroke: dir) }\n}",
        &primary,
        &[],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let scale = first_scale(&ir);
    let cm = scale.color_map.as_ref().expect("color map");
    assert_eq!(cm[0], ("A".into(), "burlywood".into()));
    let lm = scale.label_map.as_ref().expect("label map");
    assert_eq!(lm[1], ("R".into(), "Retreat".into()));
}

#[test]
fn test_scale_range_and_null_domain_bounds() {
    let primary = vec![col("x", DataType::Float), col("n", DataType::Float)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Scale(strokeWidth: n, domain: [0, null], range: [0, 30])\n  Space(x) { Point() }\n}",
        &primary,
        &[],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let scale = first_scale(&ir);
    assert_eq!(scale.domain, Some([Some(0.0), None]));
    assert_eq!(scale.range, Some([Some(0.0), Some(30.0)]));
}

#[test]
fn test_guide_axis_label_null_suppresses() {
    let primary = vec![col("x", DataType::Float), col("y", DataType::Float)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") {\n  Guide(axis: x, label: null)\n  Space(x * y) { Point() }\n}",
        &primary,
        &[],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.guides.x_label.as_deref(), Some(""));
}

#[test]
fn test_path_geometry_is_known() {
    let primary = vec![col("x", DataType::Float), col("y", DataType::Float)];
    let analysis = analyze_tables(
        "Chart(data: \"t.csv\") { Space(x * y) { Path() } }",
        &primary,
        &[],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    assert_eq!(ir.spaces[0].geometries[0].kind, GeometryKind::Path);
}

// --- Geospatial: source constructors, spatial frame, Geo mark (spec §10.11, §16.14) ---

#[test]
fn test_geojson_source_constructor_is_accepted() {
    // A `GeoJson(...)` source must not be rejected as an invalid data source.
    clean("Chart(data: GeoJson(\"us.geojson\")) {\n  Space(geom) { Geo(fill: value) }\n}");
}

#[test]
fn test_shapefile_source_constructor_is_accepted() {
    clean("Chart(data: Shapefile(\"us.shp\")) {\n  Space(geom) { Geo(stroke: \"#fff\") }\n}");
}

#[test]
fn test_named_table_geojson_source_is_accepted() {
    clean(
        "Chart(data: \"p.csv\") {\n  Table counties = GeoJson(\"us.geojson\")\n  \
         Space(flipper_length * body_mass) { Point() }\n}",
    );
}

#[test]
fn test_geo_in_spatial_space_is_clean() {
    clean("Chart(data: GeoJson(\"us.geojson\")) {\n  Space(geom, projection: \"albers_usa\") { Geo(fill: value) }\n}");
}

#[test]
fn test_geo_on_non_geometry_column_reports_e1801() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(value) { Geo(fill: amount) }\n}",
        "E1801"
    ));
}

#[test]
fn test_geo_in_planar_space_reports_e1804() {
    assert!(has(
        "Chart(data: \"p.csv\") {\n  Space(value * amount) { Geo(fill: amount) }\n}",
        "E1804"
    ));
}

#[test]
fn test_non_string_projection_reports_e1802() {
    assert!(has(
        "Chart(data: GeoJson(\"us.geojson\")) {\n  Space(geom, projection: 42) { Geo(fill: value) }\n}",
        "E1802"
    ));
}
