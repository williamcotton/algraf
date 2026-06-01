//! Formatter tests (spec §21.10, §27.1).

use algraf_syntax::format;

#[test]
fn test_canonical_minimal_chart() {
    let source = r#"Chart(data:"penguins.csv"){
Space(flipper_length*body_mass){
Point(fill:species,alpha:0.7,size:3)
}
}"#;
    let expected = "Chart(data: \"penguins.csv\") {\n    Space(flipper_length * body_mass) {\n        Point(fill: species, alpha: 0.7, size: 3)\n    }\n}\n";
    assert_eq!(format(source), expected);
}

#[test]
fn test_already_canonical_is_idempotent() {
    let canonical = "Chart(data: \"penguins.csv\") {\n    Space(flipper_length * body_mass) {\n        Point(fill: species, alpha: 0.7, size: 3)\n    }\n}\n";
    assert_eq!(format(canonical), canonical);
    // Formatting twice yields the same result.
    assert_eq!(format(&format(canonical)), canonical);
}

#[test]
fn test_operator_spacing_and_precedence_parens() {
    let source = "Chart(data:\"f.csv\"){Space((quarter/type)*amount){Bar(fill:type)}}";
    let expected = "Chart(data: \"f.csv\") {\n    Space((quarter / type) * amount) {\n        Bar(fill: type)\n    }\n}\n";
    assert_eq!(format(source), expected);
}

#[test]
fn test_indentation_is_four_spaces() {
    let formatted = format("Chart(data:\"d.csv\"){Space(a*b){Point()}}");
    let lines: Vec<&str> = formatted.lines().collect();
    assert_eq!(lines[1], "    Space(a * b) {");
    assert_eq!(lines[2], "        Point()");
}

#[test]
fn test_long_call_wraps_one_arg_per_line() {
    let source = r#"Chart(data: "distribution.csv") {
    Derive bins = Bin(value, bins: 25)

    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count, fill: "steelblue", alpha: 0.8)
    }
}"#;
    let formatted = format(source);
    // The long Rect call must wrap.
    assert!(formatted.contains("        Rect(\n"));
    assert!(formatted.contains("            xmin: bin_start,\n"));
    assert!(formatted.contains("            alpha: 0.8\n"));
    assert!(formatted.contains("        )\n"));
    // The derive is preserved on one line.
    assert!(formatted.contains("    Derive bins = Bin(value, bins: 25)\n"));
}

#[test]
fn test_quoted_columns_preserved() {
    let source =
        "Chart(data:\"d.csv\"){Space(`flipper length`*`body mass`){Point(fill:`species name`)}}";
    let formatted = format(source);
    assert!(formatted.contains("Space(`flipper length` * `body mass`) {"));
    assert!(formatted.contains("Point(fill: `species name`)"));
}

#[test]
fn test_array_value_formatting() {
    let source = "Chart(data:\"d.csv\"){Space(g*h){Violin(quantiles:[0.25,0.5,0.75])}}";
    let formatted = format(source);
    assert!(formatted.contains("Violin(quantiles: [0.25, 0.5, 0.75])"));
}

#[test]
fn test_stdin_value_formatting() {
    let formatted = format("Chart(data:stdin){Space(t*v){Line()}}");
    assert!(formatted.starts_with("Chart(data: stdin) {\n"));
}

#[test]
fn test_document_table_and_bare_chart_formatting() {
    let source = "Table main=\"p.csv\"\nChart{Space(x*y,data:main){Point()}}";
    let expected = "Table main = \"p.csv\"\n\nChart {\n    Space(x * y, data: main) {\n        Point()\n    }\n}\n";
    assert_eq!(format(source), expected);
}

#[test]
fn test_derive_from_formatting() {
    let source = "Chart(data:\"p.csv\"){Derive bins=Bin(value) Derive trend from bins=Smooth(bin_center,count)}";
    let expected = "Chart(data: \"p.csv\") {\n    Derive bins = Bin(value)\n    Derive trend from bins = Smooth(bin_center, count)\n}\n";
    assert_eq!(format(source), expected);
}

#[test]
fn test_inset_formatting() {
    let source = "Chart(data:\"p.csv\"){Table mix=\"mix.csv\" Space(x*y){Inset(data:mix,match:[id=>parent.id],size:32){Space(value,coords:\"polar\",theta:\"y\"){Bar(fill:category,layout:\"fill\")}}}}";
    let formatted = format(source);
    assert!(formatted.contains("Inset(data: mix, match: [id => parent.id], size: 32) {"));
    assert!(formatted.contains("Space(value, coords: \"polar\", theta: \"y\") {"));
}

#[test]
fn test_standalone_comment_preserved() {
    let source =
        "Chart(data: \"d.csv\") {\n    // a note\n    Space(a * b) {\n        Point()\n    }\n}";
    let formatted = format(source);
    assert!(formatted.contains("    // a note\n    Space(a * b) {"));
}

#[test]
fn test_trailing_comment_preserved() {
    let source =
        "Chart(data: \"d.csv\") {\n    Space(a * b) {\n        Point() // the points\n    }\n}";
    let formatted = format(source);
    assert!(formatted.contains("Point()  // the points\n"));
}

#[test]
fn test_standalone_block_comment_preserved() {
    let source =
        "Chart(data: \"d.csv\") {\n    /* a note */\n    Space(a * b) {\n        Point()\n    }\n}";
    let formatted = format(source);
    assert!(formatted.contains("    /* a note */\n    Space(a * b) {"));
}

#[test]
fn test_trailing_block_comment_preserved() {
    let source =
        "Chart(data: \"d.csv\") {\n    Space(a * b) {\n        Point() /* the points */\n    }\n}";
    let formatted = format(source);
    assert!(formatted.contains("Point()  /* the points */\n"));
}

#[test]
fn test_malformed_source_returned_unchanged() {
    // Syntactically invalid input is returned verbatim (spec §2296).
    let source = "Chart(data: \"d.csv\" {";
    assert_eq!(format(source), source);
}

#[test]
fn test_idempotent_on_spec_examples() {
    // Formatting is stable: format(format(x)) == format(x).
    for source in [
        "Chart(data:\"d.csv\"){Space(time*(lower+upper)){Ribbon(ymin:lower,ymax:upper)}}",
        "Chart(data:\"d.csv\"){Space(gender*height){Boxplot(fill:gender)}}",
        "Chart(data:\"d.csv\"){Space(day*hour){Tile(fill:value)}}",
    ] {
        let once = format(source);
        let twice = format(&once);
        assert_eq!(once, twice, "not idempotent for: {source}");
    }
}

#[test]
fn test_let_binding_is_formatted() {
    let source = "Chart(data:\"p.csv\"){let primary=\"#3366cc\"\nlet dim=0.4\nSpace(a*b){let local=true\nPoint(fill:primary)}}";
    let expected = "Chart(data: \"p.csv\") {\n    let primary = \"#3366cc\"\n    let dim = 0.4\n    Space(a * b) {\n        let local = true\n        Point(fill: primary)\n    }\n}\n";
    assert_eq!(format(source), expected);
}

#[test]
fn test_theme_nested_call_value_formats() {
    let source = "Chart(data:\"p.csv\"){Theme(name:\"minimal\",axisText:Text(size:12,fill:\"#333\"))\nSpace(a*b){Point()}}";
    let formatted = format(source);
    assert!(
        formatted.contains("axisText: Text(size: 12, fill: \"#333\")"),
        "{formatted}"
    );
}

#[test]
fn test_table_and_map_round_trip() {
    let source = "Chart(data:\"t.csv\"){Table cities=\"c.csv\"\nScale(stroke:dir,range:[\"A\"=>\"burlywood\",\"R\"=>\"black\"])\nSpace(x*y,data:cities){Point(stroke:dir)}}";
    let formatted = format(source);
    assert!(
        formatted.contains("Table cities = \"c.csv\""),
        "{formatted}"
    );
    assert!(
        formatted.contains("[\"A\" => \"burlywood\", \"R\" => \"black\"]"),
        "{formatted}"
    );
    // Idempotent.
    assert_eq!(format(&formatted), formatted);
}

#[test]
fn test_source_header_and_v020_calls_format() {
    let source = "Algraf(version:\"0.20\",features:[\"experimental\"])\nChart(data:\"t.csv\"){let muted=Style(fill:\"#6b7280\",alpha:0.5)\nScale(fill:value,gradient:[Stop(value:0,color:\"#3366cc\"),Stop(value:100,color:\"#cc3333\")])\nGuide(axis:x,timeFormat:\"iso-minute\")\nSpace(time*value){Point(style:muted)}}";
    let formatted = format(source);
    assert!(
        formatted.starts_with("Algraf(version: \"0.20\", features: [\"experimental\"])\n\nChart")
    );
    assert!(formatted.contains("let muted = Style(fill: \"#6b7280\", alpha: 0.5)"));
    assert!(formatted.contains("Stop(value: 0, color: \"#3366cc\")"));
    assert!(formatted.contains("Guide(axis: x, timeFormat: \"iso-minute\")"));
}
