use algraf_core::codes;

use crate::lexer::TokenKind;
use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    // --- Algebra grammar (spec §7.7, §12.5) ---

    /// Pratt parser for algebra expressions. `min_bp` is the minimum left
    /// binding power required to continue consuming operators.
    pub(super) fn algebra_expr(&mut self, min_bp: u8) {
        let checkpoint = self.builder.checkpoint();
        self.algebra_primary();

        while let Some((left_bp, right_bp)) = self.current_binding_power() {
            if left_bp < min_bp {
                break;
            }
            self.bump(); // operator token
            self.algebra_expr(right_bp);
            self.builder
                .start_node_at(checkpoint, SyntaxKind::ALGEBRA_BINARY.into());
            self.builder.finish_node();
        }
    }

    /// Binding powers per spec §12.5: nest `/` binds tightest, then cross `*`,
    /// then blend `+`. Right binding power exceeds left for left-associativity.
    pub(super) fn current_binding_power(&self) -> Option<(u8, u8)> {
        match self.current_kind() {
            TokenKind::Plus => Some((1, 2)),
            TokenKind::Star => Some((3, 4)),
            TokenKind::Slash => Some((5, 6)),
            _ => None,
        }
    }

    pub(super) fn algebra_primary(&mut self) {
        match self.current_kind().clone() {
            TokenKind::Ident(_) if self.nth_kind(1) == SyntaxKind::L_PAREN => {
                self.builder.start_node(SyntaxKind::ALGEBRA_CALL.into());
                self.bump(); // operator name
                self.bump(); // '('
                if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                    let span = self.current_span();
                    self.error(codes::E0009, "expected algebra expression", span);
                    self.builder.start_node(SyntaxKind::ERROR.into());
                    self.builder.finish_node();
                } else {
                    self.algebra_expr(0);
                }
                self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
                self.builder.finish_node();
            }
            TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => {
                self.builder.start_node(SyntaxKind::ALGEBRA_NAME.into());
                self.bump();
                self.builder.finish_node();
            }
            TokenKind::LParen => {
                self.builder.start_node(SyntaxKind::ALGEBRA_PAREN.into());
                self.bump(); // '('
                self.algebra_expr(0);
                self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
                self.builder.finish_node();
            }
            // A closing delimiter or EOF where a primary is expected: insert a
            // zero-width error node without consuming the delimiter (spec §12.6).
            TokenKind::RParen | TokenKind::RBrace | TokenKind::Comma | TokenKind::Eof => {
                let span = self.current_span();
                self.error(codes::E0009, "expected algebra expression", span);
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.builder.finish_node();
            }
            // An unrelated token: consume it into an error node (spec §12.6).
            _ => {
                let span = self.current_span();
                self.error(codes::E0009, "expected algebra expression", span);
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.bump();
                self.builder.finish_node();
            }
        }
    }
}
