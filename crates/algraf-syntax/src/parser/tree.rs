use algraf_core::{Diagnostic, DiagnosticCode, Span};

use crate::lexer::TokenKind;
use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    // --- Tree building ---

    /// Push the token at the cursor into the tree (with the given kind) and
    /// advance the raw cursor by one.
    pub(super) fn push_raw(&mut self, kind: SyntaxKind) {
        let text = self.tokens[self.pos].text.clone();
        self.builder.token(kind.into(), &text);
        self.pos += 1;
    }

    /// Attach any pending trivia tokens to the current node.
    pub(super) fn eat_trivia(&mut self) {
        while self.tokens[self.pos].kind.is_trivia() {
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
    }

    /// Consume the current significant token using its natural kind. Does
    /// nothing at EOF.
    pub(super) fn bump(&mut self) {
        self.eat_trivia();
        if matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            return;
        }
        let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
        self.push_raw(kind);
    }

    /// Consume the current significant token but tag it with `kind` (used to
    /// retag an identifier lexeme as a contextual keyword).
    pub(super) fn bump_as(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        if matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            return;
        }
        self.push_raw(kind);
    }

    /// Consume every remaining token up to EOF into an error node.
    pub(super) fn drain_into_error(&mut self) {
        self.builder.start_node(SyntaxKind::ERROR.into());
        while !matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
        self.builder.finish_node();
    }

    pub(super) fn error(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    pub(super) fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Consume the current significant token if it matches `kind`; otherwise
    /// record a diagnostic without consuming (recovery is the caller's job).
    pub(super) fn expect(&mut self, kind: SyntaxKind, code: DiagnosticCode, message: &str) {
        if self.at(kind) {
            self.bump();
        } else {
            let span = self.current_span();
            self.error(code, message, span);
        }
    }
}
