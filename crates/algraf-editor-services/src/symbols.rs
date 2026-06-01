use algraf_syntax::ast::{ChartItem, Root, SpaceItem};
use algraf_syntax::{node_span, SyntaxNode};
use lsp_types::{DocumentSymbol, SymbolKind};

use crate::positions::span_to_range;

pub fn document_symbols(source: &str, syntax: &SyntaxNode) -> Vec<DocumentSymbol> {
    let Some(root) = Root::cast(syntax.clone()) else {
        return Vec::new();
    };
    let Some(chart) = root.chart() else {
        return Vec::new();
    };
    let mut out = root
        .tables()
        .into_iter()
        .map(|decl| {
            let name = decl.name().unwrap_or_else(|| "Table".to_string());
            symbol(
                source,
                &format!("Table {name}"),
                SymbolKind::VARIABLE,
                decl.syntax(),
                Vec::new(),
            )
        })
        .collect::<Vec<_>>();

    let mut chart_symbol = symbol(
        source,
        "Chart",
        SymbolKind::OBJECT,
        chart.syntax(),
        Vec::new(),
    );
    let mut children = Vec::new();
    for item in chart.items() {
        match item {
            ChartItem::Derive(decl) => {
                let name = decl.name().unwrap_or_else(|| "Derive".to_string());
                children.push(symbol(
                    source,
                    &format!("Derive {name}"),
                    SymbolKind::VARIABLE,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Let(decl) => {
                let name = decl.name().unwrap_or_else(|| "let".to_string());
                children.push(symbol(
                    source,
                    &format!("let {name}"),
                    SymbolKind::VARIABLE,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Table(decl) => {
                let name = decl.name().unwrap_or_else(|| "Table".to_string());
                children.push(symbol(
                    source,
                    &format!("Table {name}"),
                    SymbolKind::VARIABLE,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Space(space) => {
                let mut space_children = Vec::new();
                for child in space.items() {
                    match child {
                        SpaceItem::Geometry(geometry) => {
                            let name = geometry.name().unwrap_or_else(|| "Geometry".to_string());
                            space_children.push(symbol(
                                source,
                                &name,
                                SymbolKind::FUNCTION,
                                geometry.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Inset(inset) => {
                            space_children.push(symbol(
                                source,
                                "Inset",
                                SymbolKind::OBJECT,
                                inset.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Scale(decl)
                        | SpaceItem::Guide(decl)
                        | SpaceItem::Theme(decl) => {
                            space_children.push(symbol(
                                source,
                                decl.keyword(),
                                SymbolKind::PROPERTY,
                                decl.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Let(decl) => {
                            let name = decl.name().unwrap_or_else(|| "let".to_string());
                            space_children.push(symbol(
                                source,
                                &format!("let {name}"),
                                SymbolKind::VARIABLE,
                                decl.syntax(),
                                Vec::new(),
                            ));
                        }
                        SpaceItem::Error(_) => {}
                    }
                }
                children.push(symbol(
                    source,
                    "Space",
                    SymbolKind::OBJECT,
                    space.syntax(),
                    space_children,
                ));
            }
            ChartItem::Scale(decl)
            | ChartItem::Guide(decl)
            | ChartItem::Theme(decl)
            | ChartItem::Layout(decl)
            | ChartItem::Parse(decl) => {
                children.push(symbol(
                    source,
                    decl.keyword(),
                    SymbolKind::PROPERTY,
                    decl.syntax(),
                    Vec::new(),
                ));
            }
            ChartItem::Error(_) => {}
        }
    }
    chart_symbol.children = Some(children);
    out.push(chart_symbol);
    out
}

fn symbol(
    source: &str,
    name: &str,
    kind: SymbolKind,
    node: &SyntaxNode,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    let range = span_to_range(source, node_span(node));
    #[allow(deprecated)]
    DocumentSymbol {
        name: name.to_string(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: (!children.is_empty()).then_some(children),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_syntax::parse;

    #[test]
    fn space_and_geometry_nest_under_chart() {
        let source = "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point()\n  }\n}";
        let syntax = parse(source).syntax();
        let symbols = document_symbols(source, &syntax);
        // The single top-level symbol is the Chart; Space nests under it, and
        // Point under the Space.
        assert_eq!(symbols.len(), 1);
        let chart = &symbols[0];
        assert_eq!(chart.name, "Chart");
        let chart_children = chart.children.as_ref().expect("chart children");
        let space = chart_children
            .iter()
            .find(|s| s.name == "Space")
            .expect("space symbol");
        let space_children = space.children.as_ref().expect("space children");
        assert!(space_children.iter().any(|c| c.name == "Point"));
    }

    #[test]
    fn empty_source_has_no_symbols() {
        let syntax = parse("").syntax();
        assert!(document_symbols("", &syntax).is_empty());
    }
}
