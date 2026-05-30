use algraf_core::Span;

use crate::lexer::{TokenKind, TokenWithSpan};
use crate::syntax_kind::SyntaxKind;

use super::{is_near_keyword, Parser};

impl Parser {
    // --- Token cursor (trivia-skipping lookahead) ---

    /// The `n`-th significant (non-trivia) token ahead of the cursor, clamped at
    /// the terminal EOF token.
    pub(super) fn nth(&self, n: usize) -> &TokenWithSpan {
        let mut seen = 0;
        let mut i = self.pos;
        loop {
            let tok = &self.tokens[i];
            if tok.kind.is_trivia() {
                i += 1;
                continue;
            }
            if seen == n || matches!(tok.kind, TokenKind::Eof) {
                return tok;
            }
            seen += 1;
            i += 1;
        }
    }

    pub(super) fn current_kind(&self) -> &TokenKind {
        &self.nth(0).kind
    }

    pub(super) fn current_span(&self) -> Span {
        self.nth(0).span
    }

    pub(super) fn at_eof(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Eof)
    }

    /// The syntax kind of the `n`-th significant token.
    pub(super) fn nth_kind(&self, n: usize) -> SyntaxKind {
        SyntaxKind::from_token(&self.nth(n).kind)
    }

    pub(super) fn at(&self, kind: SyntaxKind) -> bool {
        self.nth_kind(0) == kind
    }

    /// The text of the current token if it is a plain identifier.
    pub(super) fn current_ident_text(&self) -> Option<&str> {
        match self.current_kind() {
            TokenKind::Ident(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Whether the current token is the keyword `kw` (a plain identifier whose
    /// text matches exactly).
    pub(super) fn at_kw(&self, kw: &str) -> bool {
        self.current_ident_text() == Some(kw)
    }

    pub(super) fn at_misspelled_kw(&self, kw: &str) -> bool {
        self.current_ident_text()
            .is_some_and(|text| text != kw && is_near_keyword(text, kw))
    }

    /// Whether the current token begins a chart-body keyword item.
    pub(super) fn at_chart_keyword(&self) -> bool {
        matches!(
            self.current_ident_text(),
            Some(
                "Chart"
                    | "Space"
                    | "Derive"
                    | "Scale"
                    | "Guide"
                    | "Theme"
                    | "Layout"
                    | "Table"
                    | "Parse"
                    | "Algraf"
                    | "let"
            )
        )
    }

    /// Whether the current token looks like the start of an argument
    /// (`identifier ":"`), used to distinguish a stat input from arguments.
    pub(super) fn at_arg_start(&self) -> bool {
        self.at(SyntaxKind::IDENT) && self.nth_kind(1) == SyntaxKind::COLON
    }
}
