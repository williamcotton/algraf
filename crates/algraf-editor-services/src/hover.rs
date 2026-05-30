use algraf_data::{ColumnDef, DataType};
use algraf_semantics::registry;
use algraf_syntax::ast::{AlgebraExpr, SpaceBlock, ValueExpr};
use algraf_syntax::{
    node_span, parse, source_constructor_meta, tokenize, unescape_quoted_ident,
    unescape_string_literal, SyntaxKind,
};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::document::DocumentState;
use crate::positions::span_to_range;

pub fn hover_at(state: &DocumentState, offset: usize) -> Option<Hover> {
    let lexed = tokenize(&state.text);
    let tokens: Vec<_> = lexed
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let idx = tokens
        .iter()
        .position(|token| token.span.contains(offset))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.span.end == offset && token.span.start < token.span.end)
        })?;
    let token = &tokens[idx];

    use algraf_syntax::TokenKind;
    let contents = match &token.kind {
        TokenKind::Star => Some(operator_hover(
            "Cross operator",
            "`*` builds a Cartesian product between algebraic frames.",
        )),
        TokenKind::Slash => Some(operator_hover(
            "Nest operator",
            "`/` nests the right frame inside each value of the left frame.",
        )),
        TokenKind::Plus => Some(operator_hover(
            "Blend operator",
            "`+` unions compatible domains in an explicitly parenthesized blend.",
        )),
        TokenKind::Ident(name) => hover_for_ident(state, &tokens, idx, name),
        TokenKind::QuotedIdent(raw) => {
            let name = unescape_quoted_ident(raw);
            hover_for_ident(state, &tokens, idx, &name)
        }
        TokenKind::String(raw) => hover_for_string(state, raw),
        _ => None,
    }?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: contents,
        }),
        range: Some(span_to_range(&state.text, token.span)),
    })
}

fn hover_for_ident(
    state: &DocumentState,
    tokens: &[algraf_syntax::TokenWithSpan],
    idx: usize,
    name: &str,
) -> Option<String> {
    if next_significant_is_colon(tokens, idx) {
        return Some(format!(
            "**Property `{name}`**\n\n{}",
            registry::property_doc(name)
        ));
    }
    if registry::geometry(name).is_some() {
        return Some(format!(
            "**Geometry `{name}`**\n\n{}",
            registry::geometry_doc(name)
        ));
    }
    if name == "transpose" && next_significant_is_lparen(tokens, idx) {
        return Some(
            "**Frame operator `transpose`**\n\nSwaps the two axes of a two-dimensional Cartesian frame."
                .to_string(),
        );
    }
    if let Some(meta) = source_constructor_meta(name) {
        if meta.name != "Sqlite" || (state.text.contains("0.21") && state.text.contains("\"sql\""))
        {
            return Some(format!("**Source `{name}`**\n\n{}", meta.doc));
        }
    }
    match name {
        "Algraf" => {
            return Some(
                "**Algraf source header**\n\nDeclares the source language version and optional feature gates."
                    .to_string(),
            );
        }
        "Style" => {
            return Some(
                "**Style fragment**\n\nReusable property bag applied with `style:`.".to_string(),
            );
        }
        "Stop" => {
            return Some(
                "**Gradient stop**\n\nA positioned continuous color stop with `value:` and `color:`."
                    .to_string(),
            );
        }
        _ => {}
    }
    if let Some(column) =
        state
            .column_schema_at(tokens[idx].span.start)
            .and_then(|(schema, source)| {
                schema
                    .iter()
                    .find(|column| column.name == name)
                    .map(|column| (column, source))
            })
    {
        let (column, source) = column;
        let examples = if column.examples.is_empty() {
            String::new()
        } else {
            format!("\n\nExamples: `{}`", column.examples.join("`, `"))
        };
        return Some(format!(
            "**Column `{}`**\n\nType: `{}`\n\nSource: {} (provisional LSP sample){}",
            column.name,
            dtype_name(column.dtype),
            source,
            examples
        ));
    }
    None
}

trait HoverSchemaLookup {
    fn column_schema_at(&self, offset: usize) -> Option<(&[ColumnDef], String)>;
}

impl HoverSchemaLookup for DocumentState {
    fn column_schema_at(&self, offset: usize) -> Option<(&[ColumnDef], String)> {
        if let Some(table_name) = space_data_table_at(&self.text, offset) {
            if let Some(schema) = self.table_schemas.get(&table_name) {
                return Some((schema.as_slice(), format!("Table `{table_name}`")));
            }
        }
        let source = self
            .data_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "CSV schema sample".to_string());
        self.primary_schema
            .as_deref()
            .map(|schema| (schema, source))
    }
}

fn space_data_table_at(text: &str, offset: usize) -> Option<String> {
    let root = parse(text).syntax();
    let space = root
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::SPACE_BLOCK)
        .find(|node| node_span(node).contains(offset))?;
    let block = SpaceBlock::cast(space)?;
    for arg in block.args() {
        if arg.key().as_deref() != Some("data") {
            continue;
        }
        let Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) = arg.value() else {
            continue;
        };
        return name.name();
    }
    None
}

fn hover_for_string(state: &DocumentState, raw: &str) -> Option<String> {
    let value = unescape_string_literal(raw);
    if ["identity", "stack", "fill"].contains(&value.as_str()) {
        return Some(format!(
            "**Bar layout `{value}`**\n\nControls how bars resolve collisions in a shared space."
        ));
    }
    if ["lm", "loess"].contains(&value.as_str()) {
        return Some(format!(
            "**Smooth method `{value}`**\n\nControls the statistical model used for a smooth layer."
        ));
    }
    if ["iso-date", "iso-minute"].contains(&value.as_str()) {
        return Some(format!(
            "**Temporal format `{value}`**\n\nDeterministic ISO-style axis label format."
        ));
    }
    if ["minute", "hour", "day", "week", "month", "quarter", "year"].contains(&value.as_str()) {
        return Some(format!(
            "**Temporal interval `{value}`**\n\nCalendar-aware temporal bin interval."
        ));
    }
    if ["sql", "network", "plugins", "experimental"].contains(&value.as_str()) {
        let body = if value == "sql" {
            "Enables local `Sqlite(...)` sources in Algraf v0.21."
        } else {
            "Reserved feature gate; it does not enable runtime access yet."
        };
        return Some(format!("**Feature gate `{value}`**\n\n{body}"));
    }
    if state.data_path.is_some() {
        None
    } else {
        Some("String literal".to_string())
    }
}

fn next_significant_is_colon(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Eof))
        .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
}

fn next_significant_is_lparen(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Eof))
        .is_some_and(|token| matches!(token.kind, TokenKind::LParen))
}

fn operator_hover(title: &str, body: &str) -> String {
    format!("**{title}**\n\n{body}")
}

pub fn dtype_name(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Geometry => "geometry",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentState;

    fn state(text: &str) -> DocumentState {
        DocumentState {
            text: text.to_string(),
            version: 0,
            parse: None,
            analysis: None,
            primary_schema: None,
            table_schemas: Default::default(),
            data_path: None,
            virtual_files: Default::default(),
            has_external_schema_sources: false,
            diagnostics: Vec::new(),
        }
    }

    fn markdown(hover: Hover) -> String {
        match hover.contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected markup"),
        }
    }

    fn range(hover: Hover) -> lsp_types::Range {
        hover.range.expect("hover range")
    }

    fn col(name: &str, dtype: DataType, examples: &[&str]) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            dtype,
            nullable: false,
            examples: examples
                .iter()
                .map(|example| (*example).to_string())
                .collect(),
        }
    }

    #[test]
    fn hovers_geometry_name_with_registry_doc() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        let offset = text.find("Point").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Geometry `Point`"));
    }

    #[test]
    fn hovers_property_key_with_registry_doc() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(fill: x)\n  }\n}";
        let offset = text.find("fill").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Property `fill`"));
    }

    #[test]
    fn hovers_cross_operator() {
        let text = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        let offset = text.find('*').unwrap();
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Cross operator"));
    }

    #[test]
    fn hovers_transpose_frame_operator() {
        let text = "Chart(data: \"p.csv\") {\n  Space(transpose(x * y)) {\n    Point()\n  }\n}";
        let offset = text.find("transpose").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Frame operator `transpose`"));
    }

    #[test]
    fn hovers_source_constructor_with_shared_docs() {
        let text = "Chart(data: GeoJson(\"map.geojson\")) {\n  Space() { Geo() }\n}";
        let offset = text.find("GeoJson").unwrap() + 1;
        let md = markdown(hover_at(&state(text), offset).expect("hover"));
        assert!(md.contains("Source `GeoJson`"));
    }

    #[test]
    fn hovers_primary_column_type_and_examples() {
        let text = "Chart(data: \"p.csv\") {\n  Space(region * sales) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("sales", DataType::Float, &["10.5", "12"])]);
        let offset = text.find("sales").unwrap() + 1;
        let md = markdown(hover_at(&state, offset).expect("hover"));
        assert!(md.contains("Column `sales`"));
        assert!(md.contains("Type: `float`"));
        assert!(md.contains("Examples: `10.5`, `12`"));
    }

    #[test]
    fn hovers_non_ascii_primary_column_with_utf16_range() {
        let text = "Chart(data: \"p.csv\") {\n  Space(`café` * mass) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("café", DataType::Float, &["1.5"])]);
        let offset = text.find("café").unwrap() + "caf".len();
        let hover = hover_at(&state, offset).expect("hover");
        let md = markdown(hover.clone());
        assert!(md.contains("Column `café`"));
        let range = range(hover);
        assert_eq!(
            range.start,
            crate::positions::offset_to_position(text, text.find('`').unwrap())
        );
        assert_eq!(range.end.character - range.start.character, 6);
    }

    #[test]
    fn hovers_named_table_column_type_and_examples() {
        let text = "Chart(data: \"p.csv\") {\n  Table cities = \"cities.csv\"\n  Space(lat * lon, data: cities) {\n    Point()\n  }\n}";
        let mut state = state(text);
        state.primary_schema = Some(vec![col("lat", DataType::String, &["wrong"])]);
        state.table_schemas.insert(
            "cities".to_string(),
            vec![col("lat", DataType::Float, &["45.1"])],
        );
        let offset = text.find("lat *").unwrap() + 1;
        let md = markdown(hover_at(&state, offset).expect("hover"));
        assert!(md.contains("Column `lat`"));
        assert!(md.contains("Type: `float`"));
        assert!(md.contains("Source: Table `cities`"));
        assert!(md.contains("Examples: `45.1`"));
    }
}
