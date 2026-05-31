// --- Inlay hints (spec §21.17) ----------------------------------------------

use lsp_types::{InlayHint, Range};

use crate::document::DocumentState;

/// v0.39.5 moves derived-table schema inspection to hover. The provider remains
/// as an internal no-op so old clients that request it receive a stable empty
/// list while native LSP and browser demos stop advertising it.
pub fn inlay_hints_for(state: &DocumentState, range: Range) -> Vec<InlayHint> {
    let _ = (state, range);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentState;
    use lsp_types::Position;

    #[test]
    fn derived_schema_inlay_hints_are_disabled() {
        let text =
            "Chart(data: \"p.csv\") {\n  Derive d = Bin(flipper_length)\n  Space(d) { Point() }\n}";
        let state = DocumentState {
            text: text.to_string(),
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
        };
        let full = Range::new(Position::new(0, 0), Position::new(100, 0));
        let hints = inlay_hints_for(&state, full);
        assert!(hints.is_empty());
    }
}
