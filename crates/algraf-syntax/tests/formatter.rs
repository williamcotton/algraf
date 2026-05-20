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
