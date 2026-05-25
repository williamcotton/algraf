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
