use std::collections::HashSet;

use algraf_data::{ColumnDef, DataType};
use algraf_semantics::registry;
use algraf_syntax::{parse, tokenize, SOURCE_CONSTRUCTORS};
use lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, MarkupContent, MarkupKind};

use crate::document::DocumentState;
use crate::hover::dtype_name;
use crate::navigation::build_name_index;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    TopLevel,
    ChartArgs {
        active_key: Option<String>,
    },
    ChartBody,
    DeriveSource,
    SpaceArgs {
        active_key: Option<String>,
        last_kind: LastTokenKind,
    },
    SpaceBody,
    GlyphBody,
    GeometryArgs {
        geometry: Option<String>,
        active_key: Option<String>,
    },
    DeclArgs {
        decl: String,
        active_key: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastTokenKind {
    Operator(char),
    Identifier,
    Other,
}

pub fn completion_context(text: &str, offset: usize) -> CompletionContext {
    let prefix = &text[..offset.min(text.len())];
    if prefix.trim().is_empty() {
        return CompletionContext::TopLevel;
    }

    let lexed = tokenize(prefix);
    let tokens: Vec<_> = lexed
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();

    let mut calls: Vec<Option<String>> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut pending_block: Option<String> = None;
    let mut previous_ident: Option<String> = None;
    let mut call_name_stack: Vec<Option<String>> = Vec::new();

    for token in &tokens {
        use algraf_syntax::TokenKind;
        match &token.kind {
            TokenKind::Ident(name) => {
                previous_ident = Some(name.clone());
            }
            TokenKind::LParen => {
                calls.push(previous_ident.take());
                call_name_stack = calls.clone();
            }
            TokenKind::RParen => {
                if let Some(Some(name)) = calls.pop() {
                    if matches!(name.as_str(), "Chart" | "Space" | "Glyph") {
                        pending_block = Some(name);
                    }
                }
                call_name_stack = calls.clone();
            }
            TokenKind::LBrace => {
                let block = pending_block.take().or_else(|| {
                    previous_ident
                        .take()
                        .filter(|name| name.as_str() == "Chart")
                });
                blocks.push(block.unwrap_or_else(|| "unknown".to_string()));
                previous_ident = None;
            }
            TokenKind::RBrace => {
                blocks.pop();
                previous_ident = None;
            }
            TokenKind::Whitespace | TokenKind::Comment(_) | TokenKind::Eof => {}
            _ => {
                previous_ident = None;
                if !matches!(token.kind, TokenKind::LBrace) {
                    pending_block = None;
                }
            }
        }
    }

    let active_key = active_arg_key(&tokens);
    let last_kind = last_token_kind(&tokens);
    if in_derive_source_position(&tokens) {
        return CompletionContext::DeriveSource;
    }
    match call_name_stack.last().and_then(|name| name.as_deref()) {
        Some("Chart") => CompletionContext::ChartArgs { active_key },
        Some("Space") => CompletionContext::SpaceArgs {
            active_key,
            last_kind,
        },
        Some(
            "Algraf" | "Scale" | "Guide" | "Theme" | "Layout" | "Parse" | "Style" | "Stop" | "Bin"
            | "Glyph" | "Smooth" | "StepVertices" | "JitterPoints" | "VectorEndpoints"
            | "CurveSample" | "Bin2D" | "HexBin" | "ContourLines" | "ContourBands" | "Density2D"
            | "Density2DContours" | "Density2DBands" | "Distinct" | "Ecdf" | "Qq" | "Summary"
            | "SummaryBin" | "Cut" | "Summary2D" | "SummaryHex" | "IntervalSegments"
            | "IntervalRects" | "IntervalMiddles" | "Simplify" | "SpatialJoin",
        ) => CompletionContext::DeclArgs {
            decl: call_name_stack
                .last()
                .and_then(|name| name.clone())
                .unwrap_or_default(),
            active_key,
        },
        Some(name) if registry::geometry(name).is_some() => CompletionContext::GeometryArgs {
            geometry: Some(name.to_string()),
            active_key,
        },
        Some(_) => CompletionContext::GeometryArgs {
            geometry: None,
            active_key,
        },
        None => match blocks.last().map(String::as_str) {
            Some("Chart") => CompletionContext::ChartBody,
            Some("Space") => CompletionContext::SpaceBody,
            Some("Glyph") => CompletionContext::GlyphBody,
            None => CompletionContext::TopLevel,
            Some(_) => CompletionContext::Unknown,
        },
    }
}

fn active_arg_key(tokens: &[algraf_syntax::TokenWithSpan]) -> Option<String> {
    use algraf_syntax::TokenKind;
    let mut segment_start = 0usize;
    for (idx, token) in tokens.iter().enumerate().rev() {
        if matches!(token.kind, TokenKind::Comma | TokenKind::LParen) {
            segment_start = idx + 1;
            break;
        }
    }
    let segment = &tokens[segment_start..];
    for window in segment.windows(2).rev() {
        if let [name, colon] = window {
            if let (TokenKind::Ident(key), TokenKind::Colon) = (&name.kind, &colon.kind) {
                return Some(key.clone());
            }
        }
    }
    None
}

fn last_token_kind(tokens: &[algraf_syntax::TokenWithSpan]) -> LastTokenKind {
    use algraf_syntax::TokenKind;
    for token in tokens.iter().rev() {
        return match token.kind {
            TokenKind::Star => LastTokenKind::Operator('*'),
            TokenKind::Slash => LastTokenKind::Operator('/'),
            TokenKind::Plus => LastTokenKind::Operator('+'),
            TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => LastTokenKind::Identifier,
            TokenKind::Eof => continue,
            _ => LastTokenKind::Other,
        };
    }
    LastTokenKind::Other
}

pub fn completion_items(state: &DocumentState, context: CompletionContext) -> Vec<CompletionItem> {
    match context {
        CompletionContext::TopLevel => vec![
            snippet(
                "Algraf",
                "Algraf(version: \"0.21\")",
                "Optional source language header",
            ),
            snippet(
                "Chart",
                "Chart(data: \"$1\") {\n    Space($2) {\n        Point($3)\n    }\n}",
                "Root chart block",
            ),
            snippet(
                "Table",
                "Table $1 = \"$2.csv\"",
                "Document-scoped data table",
            ),
        ],
        CompletionContext::ChartArgs { active_key } => {
            if active_key.as_deref() == Some("data") {
                let mut items = vec![
                    snippet("\"data.csv\"", "\"$1.csv\"", "CSV data path"),
                    keyword("input", "Caller-provided chart data"),
                    keyword("stdin", "Caller-provided chart data alias"),
                ];
                items.extend(named_table_items(state));
                items.extend(
                    SOURCE_CONSTRUCTORS
                        .iter()
                        .filter(|meta| meta.name != "Sqlite" || sql_feature_enabled(&state.text))
                        .map(|meta| snippet(meta.name, meta.completion_snippet, meta.doc)),
                );
                items
            } else {
                registry::CHART_ARGS
                    .iter()
                    .map(|name| property(name, "Chart argument"))
                    .collect()
            }
        }
        CompletionContext::ChartBody => CHART_BODY_ITEMS
            .iter()
            .map(|name| keyword(name, "Chart body item"))
            .collect(),
        CompletionContext::DeriveSource => derived_table_items(state),
        CompletionContext::SpaceArgs {
            active_key,
            last_kind,
        } => {
            if active_key.as_deref() == Some("data") {
                return derived_table_items(state);
            }
            match last_kind {
                LastTokenKind::Identifier => operator_items(),
                LastTokenKind::Operator('/') => {
                    let mut categorical = column_items_matching(state, DataType::is_categorical);
                    categorical.extend(column_items_matching(state, |_| true));
                    dedupe_by_label(categorical)
                }
                LastTokenKind::Operator('*' | '+') | LastTokenKind::Other => {
                    let mut items = column_items_matching(state, |_| true);
                    items.push(keyword("(", "Start a parenthesized algebra expression"));
                    items
                }
                LastTokenKind::Operator(_) => column_items_matching(state, |_| true),
            }
        }
        CompletionContext::SpaceBody => {
            let mut items = registry::geometry_names()
                .map(|name| function(name, registry::geometry_doc(name)))
                .collect::<Vec<_>>();
            items.push(snippet(
                "On",
                "On(event: \"click\", emit: $1)",
                "Attach click event metadata to the preceding mark",
            ));
            items.extend(
                ["let", "Scale", "Guide", "Theme"]
                    .iter()
                    .map(|name| keyword(name, "Space-scoped declaration")),
            );
            items
        }
        CompletionContext::GlyphBody => {
            let mut items = vec![snippet(
                "Space",
                "Space($1) {\n    $2\n}",
                "Child space rendered by the glyph",
            )];
            items.extend(
                ["let", "Scale", "Guide", "Theme"]
                    .iter()
                    .map(|name| keyword(name, "Glyph-scoped declaration")),
            );
            items
        }
        CompletionContext::GeometryArgs {
            geometry,
            active_key,
        } => {
            if let Some(key) = active_key {
                return property_value_items(state, geometry.as_deref(), &key);
            }
            if geometry.as_deref() == Some("On") {
                return registry::EVENT_EMITTER_ARGS
                    .iter()
                    .map(|name| property(name, registry::property_doc(name)))
                    .collect();
            }
            if let Some(geometry) = geometry.and_then(|name| registry::geometry(&name)) {
                let mut items = geometry
                    .prop_names()
                    .map(|name| property(name, registry::property_doc(name)))
                    .collect::<Vec<_>>();
                items.push(property("style", registry::property_doc("style")));
                // Declarative interactions (spec §14.25) are not in any
                // geometry's PropSpec list; offer them on geometries that
                // support them.
                if registry::supports_interaction(geometry.kind) {
                    for name in registry::INTERACTION_PROPS {
                        items.push(property(name, registry::property_doc(name)));
                    }
                }
                items
            } else {
                all_property_items()
            }
        }
        CompletionContext::DeclArgs { decl, active_key } => {
            if let Some(key) = active_key {
                declaration_value_items(state, &decl, &key)
            } else {
                declaration_arg_items(&decl)
            }
        }
        CompletionContext::Unknown => Vec::new(),
    }
}

fn in_derive_source_position(tokens: &[algraf_syntax::TokenWithSpan]) -> bool {
    use algraf_syntax::TokenKind;
    let Some(derive_index) = tokens
        .iter()
        .rposition(|token| matches!(&token.kind, TokenKind::Ident(name) if name == "Derive"))
    else {
        return false;
    };
    let segment = &tokens[derive_index..];
    if segment
        .iter()
        .any(|token| matches!(token.kind, TokenKind::Equal))
    {
        return false;
    }
    matches!(
        segment.get(2).map(|token| &token.kind),
        Some(TokenKind::Ident(name)) if name == "from"
    )
}

fn derived_table_items(state: &DocumentState) -> Vec<CompletionItem> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .map(|ir| {
            let mut items: Vec<CompletionItem> = ir
                .derived_tables
                .iter()
                .map(|table| field(&table.name, "Derived table"))
                .collect();
            items.extend(
                ir.tables
                    .iter()
                    .map(|table| field(&table.name, "Named CSV table")),
            );
            items
        })
        .unwrap_or_default()
}

fn named_table_items(state: &DocumentState) -> Vec<CompletionItem> {
    state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
        .map(|ir| {
            ir.tables
                .iter()
                .map(|table| field(&table.name, "Named table"))
                .collect()
        })
        .unwrap_or_default()
}

fn column_items_matching(
    state: &DocumentState,
    predicate: impl Fn(DataType) -> bool,
) -> Vec<CompletionItem> {
    state
        .primary_schema
        .as_ref()
        .map(|schema| {
            schema
                .iter()
                .filter(|column| predicate(column.dtype))
                .map(column_item)
                .collect()
        })
        .unwrap_or_else(|| vec![keyword("Chart", "Schema is not available yet")])
}

fn column_item(column: &ColumnDef) -> CompletionItem {
    let insert_text = quote_identifier_if_needed(&column.name);
    CompletionItem {
        label: column.name.clone(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(format!("column: {}", dtype_name(column.dtype))),
        documentation: Some(markup(format!(
            "CSV column inferred as `{}` from a bounded LSP schema sample.",
            dtype_name(column.dtype)
        ))),
        insert_text: Some(insert_text),
        ..CompletionItem::default()
    }
}

fn property_value_items(
    state: &DocumentState,
    geometry: Option<&str>,
    property_name: &str,
) -> Vec<CompletionItem> {
    if geometry == Some("On") {
        return match property_name {
            "event" => vec![value_item("\"click\"", "Click event")],
            "emit" => column_items_matching(state, |_| true),
            _ => Vec::new(),
        };
    }

    let spec = geometry
        .and_then(registry::geometry)
        .and_then(|geometry| geometry.prop(property_name));
    let mut items = Vec::new();

    if property_name == "style" {
        items.extend(variable_items(state));
        items.push(value_item("Style()", "Inline style fragment"));
        return dedupe_by_label(items);
    }

    let accepts_columns = match spec {
        Some(spec) => spec
            .accepts
            .iter()
            .any(|accept| matches!(accept, registry::Accept::Column)),
        None => true,
    };
    if accepts_columns {
        items.extend(column_items_matching(state, |_| true));
    }

    // `let` variables resolve in property value positions (spec §9.6).
    items.extend(variable_items(state));

    if let Some(spec) = spec {
        for accept in spec.accepts {
            match accept {
                registry::Accept::Color => {
                    items.push(color("\"#4E79A7\"", "Categorical palette blue"));
                    items.push(color("\"#E15759\"", "Categorical palette red"));
                }
                registry::Accept::Enum(values) => {
                    items.extend(
                        values
                            .iter()
                            .map(|value| value_item(&format!("\"{value}\""), "String option")),
                    );
                }
                registry::Accept::Number => items.push(value_item("1", "Number literal")),
                registry::Accept::Str => items.push(value_item("\"\"", "String literal")),
                registry::Accept::Bool => {
                    items.push(value_item("true", "Boolean literal"));
                    items.push(value_item("false", "Boolean literal"));
                }
                registry::Accept::NumberArray => {
                    items.push(value_item("[1, 2, 3]", "Number array"));
                }
                registry::Accept::Column => {}
            }
        }
    }

    dedupe_by_label(items)
}

fn declaration_arg_items(decl: &str) -> Vec<CompletionItem> {
    registry::declaration_arg_names(decl)
        .iter()
        .map(|name| property(name, "Declaration argument"))
        .collect()
}

fn declaration_value_items(
    state: &DocumentState,
    decl: &str,
    active_key: &str,
) -> Vec<CompletionItem> {
    match (decl, active_key) {
        ("Algraf", "version") => vec![value_item("\"0.21\"", "Algraf v0.21 source version")],
        ("Algraf", "features") => {
            vec![value_item(
                "[\"sql\", \"network\", \"plugins\", \"experimental\"]",
                "`sql` enables local SQLite sources; other gates are reserved",
            )]
        }
        ("Theme", "name") => registry::THEME_NAMES
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Theme name"))
            .collect(),
        ("Theme", "plotTitle")
        | ("Theme", "plotSubtitle")
        | ("Theme", "plotCaption")
        | ("Theme", "axisTitle")
        | ("Theme", "axisText")
        | ("Theme", "stripText")
        | ("Theme", "legendTitle")
        | ("Theme", "legendText") => vec![value_item(
            "Text(size: 12, fill: \"#222222\")",
            "Structured text theme override",
        )],
        ("Theme", "gridMajor") | ("Theme", "gridMinor") => vec![value_item(
            "Line(stroke: \"#e6e6e6\", strokeWidth: 1)",
            "Structured line theme override",
        )],
        ("Theme", "panelBackground") => vec![value_item(
            "Rect(fill: \"#ffffff\")",
            "Structured rectangle theme override",
        )],
        ("Theme", "legendPosition") => ["right", "bottom", "top", "left"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Legend position"))
            .collect(),
        ("Theme", "grid") | ("Theme", "axes") => vec![
            value_item("true", "Boolean literal"),
            value_item("false", "Boolean literal"),
        ],
        ("Theme", "fontSize" | "titleSize" | "pointSize" | "lineWidth" | "legendSpacing") => {
            vec![value_item("12", "Number literal")]
        }
        ("Theme", "background" | "plotBackground" | "axisColor" | "gridColor" | "textColor") => {
            vec![color("\"#ffffff\"", "Color string")]
        }
        ("Theme", "fontFamily") => vec![value_item(
            "\"system-ui, sans-serif\"",
            "Font family string",
        )],
        ("Guide", "axis") | ("Scale", "axis") => {
            vec![value_item("x", "X axis"), value_item("y", "Y axis")]
        }
        ("Guide", "label") => vec![value_item("\"\"", "Axis label")],
        ("Guide", "timeFormat") => vec![
            value_item("\"iso-minute\"", "YYYY-MM-DD HH:MM labels"),
            value_item("\"iso-date\"", "YYYY-MM-DD labels"),
            value_item("\"iso-second\"", "YYYY-MM-DD HH:MM:SS labels"),
            value_item("\"iso-millis\"", "YYYY-MM-DD HH:MM:SS.sss labels"),
            value_item("\"rfc3339\"", "UTC RFC3339 labels"),
            value_item("\"year\"", "Year labels"),
            value_item("\"month\"", "Year-month labels"),
            value_item("\"time-minute\"", "HH:MM labels"),
            value_item("\"%b %-d, %Y\"", "Custom chrono-style temporal labels"),
        ],
        ("Guide", "tickLabelAngle") => vec![value_item("-45", "Tick label rotation in degrees")],
        ("Guide", "legend") | ("Guide", "grid") | ("Scale", "reverse") | ("Scale", "integer") => {
            vec![
                value_item("true", "Boolean literal"),
                value_item("false", "Boolean literal"),
            ]
        }
        ("Guide", "fill") | ("Guide", "stroke") => vec![value_item("null", "Suppress guide")],
        ("Parse", "as") => vec![
            value_item("\"date\"", "Parse as date values"),
            value_item("\"datetime\"", "Parse as datetime values"),
        ],
        ("Parse", "unit") => vec![
            value_item("\"seconds\"", "Unix epoch seconds"),
            value_item("\"milliseconds\"", "Unix epoch milliseconds"),
            value_item("\"microseconds\"", "Unix epoch microseconds"),
            value_item("\"nanoseconds\"", "Unix epoch nanoseconds"),
        ],
        ("Parse", "timezone") => vec![
            value_item("\"UTC\"", "Interpret naive datetimes as UTC"),
            value_item(
                "\"-05:00\"",
                "Interpret naive datetimes with a fixed offset",
            ),
            value_item(
                "\"America/Chicago\"",
                "Interpret naive datetimes in an IANA zone",
            ),
        ],
        ("Parse", "onError") => vec![
            value_item("\"warn\"", "Coerce failures to missing and warn (default)"),
            value_item("\"error\"", "Treat any parse failure as a blocking error"),
            value_item(
                "\"missing\"",
                "Coerce failures to missing without a warning",
            ),
        ],
        ("Parse", "anchor") => vec![value_item(
            "\"2026-01-01\"",
            "Anchor date for a time-only format",
        )],
        ("Scale", target) if registry::SCALE_AESTHETIC_TARGETS.contains(&target) => {
            column_items_matching(state, |_| true)
        }
        ("Scale", "type") => registry::SCALE_TYPE_NAMES
            .iter()
            .map(|name| value_item(&format!("\"{name}\""), "Scale type"))
            .collect(),
        ("Scale", "tickInterval") => vec![
            value_item("\"1 day\"", "Daily calendar ticks"),
            value_item("\"1 week\"", "ISO Monday week ticks"),
            value_item("\"1 month\"", "Month-start ticks"),
            value_item("\"3 months\"", "Quarterly ticks (Jan/Apr/Jul/Oct)"),
            value_item("\"1 quarter\"", "Quarterly ticks (Jan/Apr/Jul/Oct)"),
            value_item("\"6 months\"", "Half-year ticks (Jan/Jul)"),
            value_item("\"1 year\"", "Year-start ticks"),
            value_item("\"6 hours\"", "Clock ticks at 00/06/12/18"),
            value_item("\"15 minutes\"", "Quarter-hour clock ticks"),
        ],
        ("Scale", "domain") => vec![value_item("[0, 1]", "Numeric domain")],
        ("Scale", "palette") => registry::PALETTE_NAMES
            .iter()
            .map(|name| value_item(&format!("\"{name}\""), "Categorical palette"))
            .collect(),
        ("Scale", "gradient") => {
            vec![
                value_item("[\"#3366cc\", \"#cc3333\"]", "Even color gradient stops"),
                value_item(
                    "[Stop(value: 0, color: \"#3366cc\"), Stop(value: 100, color: \"#cc3333\")]",
                    "Positioned color gradient stops",
                ),
            ]
        }
        ("Stop", "value") => vec![value_item("0", "Domain value")],
        ("Stop", "color") => vec![color("\"#3366cc\"", "Gradient stop color")],
        ("Style", _) => property_value_items(state, None, active_key),
        ("Glyph", "data") => derived_table_items(state),
        ("Glyph", "key") => vec![value_item("[column]", "Glyph key columns")],
        ("Glyph", "scales") => ["shared", "local"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Glyph scale training default"))
            .collect(),
        ("Scale", "train") => ["shared", "local"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Scale training scope"))
            .collect(),
        ("Bin", "interval") => ["minute", "hour", "day", "week", "month", "quarter", "year"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Temporal bin interval"))
            .collect(),
        ("Bin", "closed") => ["left", "right"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Bin closure"))
            .collect(),
        ("Bin", "bins" | "binWidth" | "boundary") => vec![value_item("30", "Number literal")],
        ("StepVertices", "direction") => ["hv", "vh"]
            .iter()
            .map(|value| value_item(&format!("\"{value}\""), "Step direction"))
            .collect(),
        ("JitterPoints", "width" | "height") => vec![value_item("0.2", "Jitter amount")],
        ("VectorEndpoints", "lengthScale") => vec![value_item("1", "Length scale")],
        ("CurveSample", "curvature") => vec![value_item("0.35", "Curve bend amount")],
        ("CurveSample", "points") => vec![value_item("16", "Sample count")],
        ("IntervalSegments" | "IntervalRects" | "IntervalMiddles", "orientation") => {
            ["vertical", "horizontal"]
                .iter()
                .map(|value| value_item(&format!("\"{value}\""), "Interval orientation"))
                .collect()
        }
        ("IntervalSegments", "capWidth") => vec![value_item("0.4", "Cap width")],
        ("IntervalRects" | "IntervalMiddles", "width") => vec![value_item("0.8", "Width")],
        _ => Vec::new(),
    }
}

fn sql_feature_enabled(text: &str) -> bool {
    let root = parse(text).syntax();
    let Some(header) = algraf_syntax::ast::Root::cast(root).and_then(|root| root.source_header())
    else {
        return false;
    };
    let mut version_ok = false;
    let mut feature_ok = false;
    for arg in header.args() {
        match arg.key().as_deref() {
            Some("version") => {
                version_ok = arg
                    .value()
                    .and_then(|value| match value {
                        algraf_syntax::ast::ValueExpr::Literal(lit)
                            if lit.kind() == Some(algraf_syntax::ast::LiteralKind::String) =>
                        {
                            Some(algraf_syntax::unescape_string_literal(
                                &lit.text().unwrap_or_default(),
                            ))
                        }
                        _ => None,
                    })
                    .is_some_and(|version| version == "0.21" || version == "0.21.0");
            }
            Some("features") => {
                feature_ok = arg
                    .value()
                    .and_then(|value| match value {
                        algraf_syntax::ast::ValueExpr::Array(array) => Some(array),
                        _ => None,
                    })
                    .is_some_and(|array| {
                        array.values().iter().any(|value| match value {
                            algraf_syntax::ast::ValueExpr::Literal(lit)
                                if lit.kind() == Some(algraf_syntax::ast::LiteralKind::String) =>
                            {
                                algraf_syntax::unescape_string_literal(
                                    &lit.text().unwrap_or_default(),
                                ) == "sql"
                            }
                            _ => false,
                        })
                    });
            }
            _ => {}
        }
    }
    version_ok && feature_ok
}

fn all_property_items() -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for geometry in registry::geometry_names().filter_map(registry::geometry) {
        for name in geometry.prop_names() {
            if seen.insert(name) {
                items.push(property(name, registry::property_doc(name)));
            }
        }
    }
    if seen.insert("style") {
        items.push(property("style", registry::property_doc("style")));
    }
    items
}

fn operator_items() -> Vec<CompletionItem> {
    vec![
        operator("*", "Cross operator: builds Cartesian dimensions"),
        operator(
            "/",
            "Nest operator: nests the right frame inside the left frame",
        ),
        operator("+", "Blend operator: unions compatible frame domains"),
    ]
}

fn dedupe_by_label(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.label.clone()))
        .collect()
}

const CHART_BODY_ITEMS: &[&str] = &[
    "let", "Derive", "Table", "Parse", "Space", "Scale", "Guide", "Theme", "Layout", "Glyph",
];

/// Variable-name completions for every in-document `let` binding (spec §9.6).
fn variable_items(state: &DocumentState) -> Vec<CompletionItem> {
    let root = parse(&state.text).syntax();
    let index = build_name_index(&root);
    let mut names: Vec<String> = index.lets.iter().map(|site| site.name.clone()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| field(&name, "let binding"))
        .collect()
}

fn keyword(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn function(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn property(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn field(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn value_item(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::VALUE),
        insert_text: Some(label.to_string()),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn color(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::COLOR),
        insert_text: Some(label.to_string()),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn operator(label: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        insert_text: Some(format!(" {label} ")),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

fn snippet(label: &str, insert_text: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        insert_text: Some(insert_text.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(markup(doc)),
        ..CompletionItem::default()
    }
}

pub fn markup(content: impl Into<String>) -> lsp_types::Documentation {
    lsp_types::Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: content.into(),
    })
}
pub fn quote_identifier_if_needed(name: &str) -> String {
    if is_plain_identifier(name) {
        return name.to_string();
    }
    let mut quoted = String::from("`");
    for ch in name.chars() {
        match ch {
            '`' => quoted.push_str("\\`"),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('`');
    quoted
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentState;

    fn empty_state() -> DocumentState {
        DocumentState {
            text: String::new(),
            version: 0,
            parse: None,
            analysis: None,
            primary_schema: None,
            table_schemas: Default::default(),
            source_previews: Default::default(),
            data_path: None,
            virtual_files: Default::default(),
            has_external_schema_sources: false,
            diagnostics: Vec::new(),
        }
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn context_inside_geometry_call_resolves_geometry_args() {
        let source = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(";
        match completion_context(source, source.len()) {
            CompletionContext::GeometryArgs { geometry, .. } => {
                assert_eq!(geometry.as_deref(), Some("Point"));
            }
            other => panic!("unexpected context: {other:?}"),
        }
    }

    #[test]
    fn bare_chart_block_context_is_chart_body() {
        let source = "Chart {\n  ";
        assert_eq!(
            completion_context(source, source.len()),
            CompletionContext::ChartBody
        );
    }

    #[test]
    fn derive_from_context_is_source_table_completion() {
        let source = "Chart(data: \"p.csv\") {\n  Derive trend from ";
        assert_eq!(
            completion_context(source, source.len()),
            CompletionContext::DeriveSource
        );
    }

    #[test]
    fn space_args_completion_does_not_offer_transpose() {
        let items = completion_items(
            &empty_state(),
            CompletionContext::SpaceArgs {
                active_key: None,
                last_kind: LastTokenKind::Other,
            },
        );
        assert!(!labels(&items).contains(&"transpose"));
    }

    #[test]
    fn space_body_completion_offers_on_event_emitter() {
        let items = completion_items(&empty_state(), CompletionContext::SpaceBody);
        assert!(labels(&items).contains(&"On"));
    }

    #[test]
    fn on_arg_completion_offers_event_and_emit() {
        let items = completion_items(
            &empty_state(),
            CompletionContext::GeometryArgs {
                geometry: Some("On".to_string()),
                active_key: None,
            },
        );
        assert_eq!(labels(&items), vec!["event", "emit"]);
    }

    #[test]
    fn on_event_value_completion_offers_click() {
        let items = completion_items(
            &empty_state(),
            CompletionContext::GeometryArgs {
                geometry: Some("On".to_string()),
                active_key: Some("event".to_string()),
            },
        );
        assert_eq!(labels(&items), vec!["\"click\""]);
    }

    #[test]
    fn scale_type_completion_offers_categorical() {
        let items = completion_items(
            &empty_state(),
            CompletionContext::DeclArgs {
                decl: "Scale".to_string(),
                active_key: Some("type".to_string()),
            },
        );
        assert!(labels(&items).contains(&"\"categorical\""));
    }

    #[test]
    fn data_arg_completion_offers_source_constructors() {
        let items = completion_items(
            &empty_state(),
            CompletionContext::ChartArgs {
                active_key: Some("data".to_string()),
            },
        );
        let labels = labels(&items);
        assert!(labels.contains(&"GeoJson"));
        assert!(labels.contains(&"Shapefile"));
        assert!(labels.contains(&"TopoJson"));
        assert!(!labels.contains(&"Sqlite"));
        assert!(labels.contains(&"stdin"));
    }

    #[test]
    fn data_arg_completion_offers_sqlite_when_sql_gate_is_enabled() {
        let mut state = empty_state();
        state.text = "Algraf(version: \"0.21\", features: [\"sql\"])\nChart(data: )".to_string();
        let items = completion_items(
            &state,
            CompletionContext::ChartArgs {
                active_key: Some("data".to_string()),
            },
        );
        assert!(labels(&items).contains(&"Sqlite"));
    }

    #[test]
    fn top_level_completion_offers_chart() {
        let items = completion_items(&empty_state(), CompletionContext::TopLevel);
        assert_eq!(labels(&items), vec!["Algraf", "Chart", "Table"]);
    }
}
