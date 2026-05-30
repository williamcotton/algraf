use algraf_core::codes;

use crate::lexer::TokenKind;
use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    // --- Values (spec §7.8, §12.13–12.14) ---

    pub(super) fn value(&mut self) {
        match self.nth_kind(0) {
            SyntaxKind::STRING
            | SyntaxKind::NUMBER
            | SyntaxKind::TRUE_KW
            | SyntaxKind::FALSE_KW
            | SyntaxKind::NULL_KW => {
                self.builder.start_node(SyntaxKind::LITERAL.into());
                self.bump();
                self.builder.finish_node();
            }
            SyntaxKind::L_BRACKET => self.bracket_value(),
            // Bare `input`/`stdin` is the caller-provided data sentinel only when
            // it is the whole value (spec §10.1, §12.13); otherwise it is an
            // ordinary column.
            SyntaxKind::IDENT
                if (self.at_kw("input") || self.at_kw("stdin")) && self.value_ends_after(1) =>
            {
                self.builder.start_node(SyntaxKind::STDIN_VALUE.into());
                self.bump_as(SyntaxKind::STDIN_KW);
                self.builder.finish_node();
            }
            // A bare identifier immediately followed by `(` is a nested call
            // value, e.g. `axisText: Text(size: 12)` (spec §7.8, §20.8).
            SyntaxKind::IDENT if self.nth_kind(1) == SyntaxKind::L_PAREN => self.call_value(),
            SyntaxKind::IDENT | SyntaxKind::QUOTED_IDENT | SyntaxKind::L_PAREN => {
                self.algebra_expr(0);
            }
            _ => {
                let span = self.current_span();
                self.error(codes::E0005, "expected argument value", span);
                self.builder.start_node(SyntaxKind::ERROR.into());
                if !self.value_terminator() && !self.at_eof() {
                    self.bump();
                }
                self.builder.finish_node();
            }
        }
    }

    /// Whether the `n`-th significant token ends a value (so the preceding token
    /// is a complete value rather than the start of an algebra expression).
    pub(super) fn value_ends_after(&self, n: usize) -> bool {
        matches!(
            self.nth_kind(n),
            SyntaxKind::COMMA | SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::EOF
        )
    }

    pub(super) fn value_terminator(&self) -> bool {
        matches!(
            self.nth_kind(0),
            SyntaxKind::R_PAREN
                | SyntaxKind::R_BRACE
                | SyntaxKind::R_BRACKET
                | SyntaxKind::COMMA
                | SyntaxKind::COLON
        )
    }

    /// Parse a nested call value `Name(args)` (spec §7.8, §20.8).
    pub(super) fn call_value(&mut self) {
        self.builder.start_node(SyntaxKind::CALL_VALUE.into());
        self.bump(); // call name
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after call name",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.builder.finish_node();
    }

    /// Parse a bracketed value, dispatching to an array or a map literal. The
    /// two are distinguished by the presence of a top-level `=>` between the
    /// brackets (spec §7.8).
    pub(super) fn bracket_value(&mut self) {
        if self.bracket_is_map() {
            self.map_value();
        } else {
            self.array_value();
        }
    }

    /// Whether the bracket beginning at the cursor contains a top-level `=>`,
    /// marking it as a map literal rather than an array.
    pub(super) fn bracket_is_map(&self) -> bool {
        let mut depth = 0i32;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LBracket | TokenKind::LParen | TokenKind::LBrace => depth += 1,
                TokenKind::RBracket | TokenKind::RParen | TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::FatArrow if depth == 1 => return true,
                TokenKind::Eof => break,
                _ => {}
            }
            i += 1;
        }
        false
    }

    pub(super) fn array_value(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_VALUE.into());
        self.bump(); // '['
        loop {
            if self.at(SyntaxKind::R_BRACKET) || self.at_eof() {
                break;
            }
            if self.at(SyntaxKind::COMMA) {
                let span = self.current_span();
                self.error(codes::E0015, "unexpected ','", span);
                self.bump();
                continue;
            }
            let before = self.pos;
            self.value();
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                continue;
            }
            if self.at(SyntaxKind::R_BRACKET) || self.at_eof() {
                break;
            }
            // Missing comma: recover by continuing with the next value.
            if self.pos == before {
                break;
            }
        }
        self.expect(SyntaxKind::R_BRACKET, codes::E0015, "expected ',' or ']'");
        self.builder.finish_node();
    }

    /// Parse a map literal `[ key => value, ... ]` (spec §7.8). Each entry is a
    /// `MAP_ENTRY` node holding a key value, a `=>`, and a value.
    pub(super) fn map_value(&mut self) {
        self.builder.start_node(SyntaxKind::MAP_VALUE.into());
        self.bump(); // '['
        loop {
            if self.at(SyntaxKind::R_BRACKET) || self.at_eof() {
                break;
            }
            if self.at(SyntaxKind::COMMA) {
                let span = self.current_span();
                self.error(codes::E0021, "unexpected ',' in map literal", span);
                self.bump();
                continue;
            }
            let before = self.pos;
            self.map_entry();
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                continue;
            }
            if self.at(SyntaxKind::R_BRACKET) || self.at_eof() {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        self.expect(SyntaxKind::R_BRACKET, codes::E0021, "expected ',' or ']'");
        self.builder.finish_node();
    }

    pub(super) fn map_entry(&mut self) {
        self.builder.start_node(SyntaxKind::MAP_ENTRY.into());
        self.value(); // key
        self.expect(
            SyntaxKind::FAT_ARROW,
            codes::E0021,
            "expected '=>' in map literal entry",
        );
        self.value(); // value
        self.builder.finish_node();
    }
}
