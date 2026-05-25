//! Shared source-expression and literal helpers.
//!
//! These helpers sit with syntax because they interpret already-parsed Algraf
//! lexemes and AST nodes, but do not load files or know about runtime data
//! backends.

use algraf_core::Span;

use crate::ast::{Arg, CallValue, ChartBlock, ChartItem, LiteralKind, Root, TableDecl, ValueExpr};
use crate::{SyntaxNode, SyntaxToken};

/// A source constructor that selects a data loader explicitly (spec §10.11).
///
/// This is a syntax-level identity for a recognized constructor; the driver maps
/// it to a concrete data-loader format. New constructors are added to
/// [`SOURCE_CONSTRUCTORS`], not by widening accepted syntax elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    GeoJson,
    Shapefile,
}

/// How a source constructor's path argument must be written (spec §10.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathArgRule {
    /// Exactly one positional string-literal path argument.
    SingleStringLiteral,
}

/// Static metadata describing a recognized source constructor (spec §10.11).
///
/// The table is the single authority for recognized constructor names, their
/// format policy, path-argument rules, documentation, and editor completion
/// text. It is intentionally closed: adding `Sqlite(...)` later means adding an
/// entry here, not accepting arbitrary runtime strings as constructors.
#[derive(Debug, Clone, Copy)]
pub struct SourceConstructorMeta {
    /// The explicit loader format this constructor selects.
    pub format: SourceFormat,
    /// The constructor's authoritative source spelling (e.g. `"GeoJson"`).
    pub name: &'static str,
    /// How the path argument must be written.
    pub path_arg: PathArgRule,
    /// Human-facing documentation, shared by LSP completion and hover.
    pub doc: &'static str,
    /// The LSP completion snippet body for this constructor.
    pub completion_snippet: &'static str,
}

/// Every recognized source constructor, in declaration order.
pub const SOURCE_CONSTRUCTORS: &[SourceConstructorMeta] = &[
    SourceConstructorMeta {
        format: SourceFormat::GeoJson,
        name: "GeoJson",
        path_arg: PathArgRule::SingleStringLiteral,
        doc: "Load a GeoJSON FeatureCollection as the data source.",
        completion_snippet: "GeoJson(\"$1\")",
    },
    SourceConstructorMeta {
        format: SourceFormat::Shapefile,
        name: "Shapefile",
        path_arg: PathArgRule::SingleStringLiteral,
        doc: "Load an ESRI shapefile bundle as the data source.",
        completion_snippet: "Shapefile(\"$1\")",
    },
];

impl SourceFormat {
    /// The shared metadata for this format.
    pub fn meta(self) -> &'static SourceConstructorMeta {
        SOURCE_CONSTRUCTORS
            .iter()
            .find(|meta| meta.format == self)
            .expect("every SourceFormat has constructor metadata")
    }

    /// The Algraf constructor name for this explicit source format.
    pub fn constructor_name(self) -> &'static str {
        self.meta().name
    }

    /// Resolve a recognized source constructor name to its format.
    pub fn from_constructor_name(name: &str) -> Option<SourceFormat> {
        SOURCE_CONSTRUCTORS
            .iter()
            .find(|meta| meta.name == name)
            .map(|meta| meta.format)
    }

    /// All recognized source constructor names.
    pub fn constructor_names() -> impl Iterator<Item = &'static str> {
        SOURCE_CONSTRUCTORS.iter().map(|meta| meta.name)
    }
}

/// Resolve a recognized source constructor's metadata by name.
pub fn source_constructor_meta(name: &str) -> Option<&'static SourceConstructorMeta> {
    SOURCE_CONSTRUCTORS.iter().find(|meta| meta.name == name)
}

/// The source expression forms currently understood by `Chart(data:)` and
/// chart-scoped `Table` declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceExpr {
    /// A path source. `format: None` means choose the loader by extension.
    Path {
        path: String,
        format: Option<SourceFormat>,
        span: Span,
    },
    /// The bare `stdin` sentinel.
    Stdin { span: Span },
    /// No source expression was present.
    Missing,
    /// A syntactically present value that is not a valid source expression.
    Invalid { span: Span },
}

impl SourceExpr {
    /// The source span if a concrete source node exists.
    pub fn span(&self) -> Option<Span> {
        match self {
            SourceExpr::Path { span, .. }
            | SourceExpr::Stdin { span }
            | SourceExpr::Invalid { span } => Some(*span),
            SourceExpr::Missing => None,
        }
    }

    /// Whether this source is a path.
    pub fn is_path(&self) -> bool {
        matches!(self, SourceExpr::Path { .. })
    }

    /// Whether this source is `stdin`.
    pub fn is_stdin(&self) -> bool {
        matches!(self, SourceExpr::Stdin { .. })
    }

    /// Whether this source is missing.
    pub fn is_missing(&self) -> bool {
        matches!(self, SourceExpr::Missing)
    }
}

/// A recognized source-constructor call with its path argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceConstructor {
    pub format: SourceFormat,
    pub path: String,
    pub span: Span,
}

/// The byte span of a syntax node's significant tokens (spec §11.2).
///
/// The lossless CST preserves leading/trailing trivia inside many nodes. For
/// diagnostics, underlining that trivia makes editor ranges spill backward onto
/// previous lines, so diagnostics use the trimmed code span.
pub fn node_span(node: &SyntaxNode) -> Span {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let Some(first) = tokens.next() else {
        let range = node.text_range();
        return Span::new(
            u32::from(range.start()) as usize,
            u32::from(range.end()) as usize,
        );
    };
    let last = tokens.last().unwrap_or_else(|| first.clone());
    Span::new(token_start(&first), token_end(&last))
}

fn token_start(token: &SyntaxToken) -> usize {
    u32::from(token.text_range().start()) as usize
}

fn token_end(token: &SyntaxToken) -> usize {
    u32::from(token.text_range().end()) as usize
}

/// Strip surrounding double quotes and resolve string-literal escapes.
pub fn unescape_string_literal(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Unescape a backtick-quoted column identifier lexeme (spec §6.7).
///
/// Strips the surrounding backticks and resolves `` \` `` and `\\`. This is
/// deliberately distinct from double-quoted string unescaping.
pub fn unescape_quoted_ident(raw: &str) -> String {
    let mut chars = raw.chars().peekable();
    if chars.peek() == Some(&'`') {
        chars.next();
    }
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '`' if chars.peek().is_none() => break,
            '\\' => match chars.peek() {
                Some('`') | Some('\\') => out.push(chars.next().unwrap()),
                _ => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

/// Whether a call value is a recognized source constructor by name.
pub fn is_source_constructor(call: &CallValue) -> bool {
    call.name()
        .and_then(|name| SourceFormat::from_constructor_name(&name))
        .is_some()
}

/// Extract the path and explicit format from a recognized source constructor's
/// first positional string-literal argument.
pub fn source_constructor(call: &CallValue) -> Option<SourceConstructor> {
    let format = call
        .name()
        .and_then(|name| SourceFormat::from_constructor_name(&name))?;
    let arg = call.args().into_iter().find(|arg| arg.key().is_none())?;
    match arg.value() {
        Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
            Some(SourceConstructor {
                format,
                path: unescape_string_literal(&lit.text().unwrap_or_default()),
                span: node_span(call.syntax()),
            })
        }
        _ => None,
    }
}

/// Extract the path from a recognized source constructor.
pub fn source_constructor_path(call: &CallValue) -> Option<String> {
    source_constructor(call).map(|source| source.path)
}

/// Extract the first chart's declared data source from a parsed tree.
pub fn document_data_source(root: &SyntaxNode) -> SourceExpr {
    let Some(chart) = Root::cast(root.clone()).and_then(|root| root.chart()) else {
        return SourceExpr::Missing;
    };
    chart_data_source(&chart)
}

/// Extract one chart block's declared `data:` source.
pub fn chart_data_source(chart: &ChartBlock) -> SourceExpr {
    for arg in chart.args() {
        if arg.key().as_deref() == Some("data") {
            return source_expr_from_arg(&arg, true);
        }
    }
    SourceExpr::Missing
}

/// Extract one `Table name = <source>` declaration's source.
pub fn table_data_source(decl: &TableDecl) -> SourceExpr {
    source_expr_from_value(decl.source(), false)
}

/// Extract all named table source declarations in a chart.
pub fn chart_table_sources(chart: &ChartBlock) -> Vec<(String, SourceExpr)> {
    let mut out = Vec::new();
    for item in chart.items() {
        let ChartItem::Table(decl) = item else {
            continue;
        };
        let Some(name) = decl.name() else { continue };
        out.push((name, table_data_source(&decl)));
    }
    out
}

/// Extract a source expression from an argument value. A present argument with
/// no value is treated as invalid at the argument span.
pub fn source_expr_from_arg(arg: &Arg, allow_stdin: bool) -> SourceExpr {
    match source_expr_from_value(arg.value(), allow_stdin) {
        SourceExpr::Missing => SourceExpr::Invalid {
            span: node_span(arg.syntax()),
        },
        source => source,
    }
}

/// Extract a source expression from an optional value.
pub fn source_expr_from_value(value: Option<ValueExpr>, allow_stdin: bool) -> SourceExpr {
    match value {
        Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
            SourceExpr::Path {
                path: unescape_string_literal(&lit.text().unwrap_or_default()),
                format: None,
                span: lit.token_span().unwrap_or_else(|| node_span(lit.syntax())),
            }
        }
        Some(ValueExpr::Stdin(stdin)) if allow_stdin => SourceExpr::Stdin {
            span: node_span(stdin.syntax()),
        },
        Some(ValueExpr::Call(call)) if is_source_constructor(&call) => {
            match source_constructor(&call) {
                Some(source) => SourceExpr::Path {
                    path: source.path,
                    format: Some(source.format),
                    span: source.span,
                },
                None => SourceExpr::Invalid {
                    span: node_span(call.syntax()),
                },
            }
        }
        Some(value) => SourceExpr::Invalid {
            span: node_span(value.syntax()),
        },
        None => SourceExpr::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn first_chart(source: &str) -> ChartBlock {
        Root::cast(parse(source).syntax())
            .and_then(|root| root.chart())
            .unwrap()
    }

    #[test]
    fn source_constructor_metadata_round_trips_by_name_and_format() {
        for meta in SOURCE_CONSTRUCTORS {
            assert_eq!(
                SourceFormat::from_constructor_name(meta.name),
                Some(meta.format)
            );
            assert_eq!(meta.format.constructor_name(), meta.name);
            assert_eq!(meta.format.meta().name, meta.name);
            assert!(source_constructor_meta(meta.name).is_some());
        }
        assert!(SourceFormat::from_constructor_name("Sqlite").is_none());
        assert!(source_constructor_meta("Csv").is_none());
    }

    #[test]
    fn string_literal_unescape_handles_supported_escapes() {
        assert_eq!(
            unescape_string_literal(r#""a\nb\rc\t\"d\\e\q""#),
            "a\nb\rc\t\"d\\eq"
        );
    }

    #[test]
    fn quoted_identifier_unescape_stays_distinct() {
        assert_eq!(unescape_quoted_ident(r"`a\`b\\c\n`"), "a`b\\c\\n");
    }

    #[test]
    fn chart_string_source_uses_extension_format() {
        let chart = first_chart(r#"Chart(data: "data.csv") {}"#);
        assert!(matches!(
            chart_data_source(&chart),
            SourceExpr::Path {
                path,
                format: None,
                ..
            } if path == "data.csv"
        ));
    }

    #[test]
    fn source_constructor_extracts_explicit_format() {
        let chart = first_chart(r#"Chart(data: GeoJson("map.data")) {}"#);
        assert!(matches!(
            chart_data_source(&chart),
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::GeoJson),
                ..
            } if path == "map.data"
        ));
    }

    #[test]
    fn table_source_uses_same_extractor() {
        let chart =
            first_chart(r#"Chart(data: "primary.csv") { Table counties = Shapefile("tiny.shp") }"#);
        let tables = chart_table_sources(&chart);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].0, "counties");
        assert!(matches!(
            &tables[0].1,
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::Shapefile),
                ..
            } if path == "tiny.shp"
        ));
    }

    #[test]
    fn malformed_constructor_is_invalid() {
        let chart = first_chart(r#"Chart(data: GeoJson(path: "map.geojson")) {}"#);
        assert!(matches!(
            chart_data_source(&chart),
            SourceExpr::Invalid { .. }
        ));
    }

    #[test]
    fn stdin_is_only_valid_when_allowed() {
        let chart = first_chart(r#"Chart(data: stdin) { Table bad = stdin }"#);
        assert!(matches!(
            chart_data_source(&chart),
            SourceExpr::Stdin { .. }
        ));
        let tables = chart_table_sources(&chart);
        assert!(matches!(tables[0].1, SourceExpr::Invalid { .. }));
    }
}
