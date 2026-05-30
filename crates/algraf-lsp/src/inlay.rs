// --- Inlay hints (spec §21.17) ----------------------------------------------

use algraf_syntax::ast::DeriveDecl;
use algraf_syntax::{node_span, parse, SyntaxKind};
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Range};

use crate::document::DocumentState;
use crate::hover::dtype_name;
use crate::positions::{offset_to_position, range_to_offsets};

/// Inlay hints showing the output columns each in-document `Derive` produces
/// (e.g. `bin_start`, `bin_end`, `bin_center`, `count`).
pub(crate) fn inlay_hints_for(state: &DocumentState, range: Range) -> Vec<InlayHint> {
    let Some(ir) = state
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.ir.as_ref())
    else {
        return Vec::new();
    };
    let (range_start, range_end) = match range_to_offsets(&state.text, range) {
        Some(offsets) => offsets,
        None => (0, state.text.len()),
    };

    let root = parse(&state.text).syntax();
    let mut hints = Vec::new();
    for node in root.descendants() {
        if node.kind() != SyntaxKind::DERIVE_DECL {
            continue;
        }
        let Some(decl) = DeriveDecl::cast(node.clone()) else {
            continue;
        };
        let Some(name) = decl.name() else { continue };
        let span = node_span(&node);
        if span.end < range_start || span.start > range_end {
            continue;
        }
        let Some(table) = ir.derived_tables.iter().find(|table| table.name == name) else {
            continue;
        };
        if table.output_schema.is_empty() {
            continue;
        }
        let columns = table
            .output_schema
            .iter()
            .map(|col| format!("{}: {}", col.name, dtype_name(col.dtype)))
            .collect::<Vec<_>>()
            .join(", ");
        hints.push(InlayHint {
            position: offset_to_position(&state.text, span.end),
            label: InlayHintLabel::String(format!(" → {columns}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        });
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{AnalysisState, DocumentState};
    use algraf_data::{ColumnDef, DataType};
    use algraf_semantics::analyze_with_tables;
    use std::collections::HashMap;
    use tower_lsp::lsp_types::Position;

    fn col(name: &str, dtype: DataType) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            dtype,
            nullable: false,
            examples: vec![],
        }
    }

    #[test]
    fn derive_inlay_hint_lists_output_columns() {
        let text =
            "Chart(data: \"p.csv\") {\n  Derive d = Bin(flipper_length)\n  Space(d) { Point() }\n}";
        let schema = vec![col("flipper_length", DataType::Float)];
        let analysis = analyze_with_tables(&parse(text).syntax(), &schema, &HashMap::new());
        let state = DocumentState {
            text: text.to_string(),
            version: 0,
            parse: None,
            analysis: Some(AnalysisState {
                ir: analysis.ir,
                diagnostics: analysis.diagnostics,
            }),
            primary_schema: Some(schema),
            table_schemas: Default::default(),
            data_path: None,
            has_external_schema_sources: false,
            diagnostics: Vec::new(),
        };
        let full = Range::new(Position::new(0, 0), Position::new(100, 0));
        let hints = inlay_hints_for(&state, full);
        assert_eq!(hints.len(), 1);
        let InlayHintLabel::String(label) = &hints[0].label else {
            panic!("expected string label");
        };
        assert!(label.contains("bin_start"));
        assert!(label.contains("count"));
    }
}
