//! Parser (spec §12). Recursive descent for blocks and calls, Pratt parsing
//! for algebra expressions, building a lossless rowan CST plus diagnostics.
//!
//! The parser is resilient: it never panics, always advances on error, records
//! diagnostics with spans, and recovers locally so a single mistake does not
//! discard later valid blocks (spec §12.1, §12.16, §12.17).

use algraf_core::{Diagnostic, Span};
use rowan::{GreenNode, GreenNodeBuilder};

use crate::lexer::{tokenize, TokenKind, TokenWithSpan};
use crate::syntax_kind::{SyntaxKind, SyntaxNode};

/// The result of a parse: a lossless green tree plus parse diagnostics.
#[derive(Debug, Clone)]
pub struct Parse {
    green: GreenNode,
    diagnostics: Vec<Diagnostic>,
}

impl Parse {
    /// The root syntax node of the parsed tree.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Parse and lexical diagnostics gathered during this parse.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consume the parse, returning its diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Parse a full Algraf source document into a [`SyntaxKind::ROOT`] tree
/// containing one chart block (spec §7.1).
pub fn parse(source: &str) -> Parse {
    let lexed = tokenize(source);
    let mut parser = Parser::new(lexed.tokens, lexed.diagnostics);

    parser.builder.start_node(SyntaxKind::ROOT.into());
    parser.eat_trivia();
    parser.program();
    parser.builder.finish_node();

    Parse {
        green: parser.builder.finish(),
        diagnostics: parser.diagnostics,
    }
}

/// Parse a standalone algebra expression, wrapping it in a [`SyntaxKind::ROOT`]
/// node. Useful for testing the algebra grammar in isolation; the block parser
/// reuses [`Parser::algebra_expr`].
pub fn parse_algebra(source: &str) -> Parse {
    let lexed = tokenize(source);
    let mut parser = Parser::new(lexed.tokens, lexed.diagnostics);

    parser.builder.start_node(SyntaxKind::ROOT.into());
    parser.eat_trivia();
    parser.algebra_expr(0);
    parser.eat_trivia();
    if !parser.at_eof() {
        let span = parser.current_span();
        parser.error("E0011", "unexpected token after algebra expression", span);
        parser.drain_into_error();
    }
    parser.builder.finish_node();

    Parse {
        green: parser.builder.finish(),
        diagnostics: parser.diagnostics,
    }
}

struct Parser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn new(tokens: Vec<TokenWithSpan>, diagnostics: Vec<Diagnostic>) -> Self {
        Parser {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            diagnostics,
        }
    }

    // --- Token cursor (trivia-skipping lookahead) ---

    /// The `n`-th significant (non-trivia) token ahead of the cursor, clamped at
    /// the terminal EOF token.
    fn nth(&self, n: usize) -> &TokenWithSpan {
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

    fn current_kind(&self) -> &TokenKind {
        &self.nth(0).kind
    }

    fn current_span(&self) -> Span {
        self.nth(0).span
    }

    fn at_eof(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Eof)
    }

    /// The syntax kind of the `n`-th significant token.
    fn nth_kind(&self, n: usize) -> SyntaxKind {
        SyntaxKind::from_token(&self.nth(n).kind)
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.nth_kind(0) == kind
    }

    /// The text of the current token if it is a plain identifier.
    fn current_ident_text(&self) -> Option<&str> {
        match self.current_kind() {
            TokenKind::Ident(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Whether the current token is the keyword `kw` (a plain identifier whose
    /// text matches exactly).
    fn at_kw(&self, kw: &str) -> bool {
        self.current_ident_text() == Some(kw)
    }

    /// Whether the current token begins a chart-body keyword item.
    fn at_chart_keyword(&self) -> bool {
        matches!(
            self.current_ident_text(),
            Some("Chart" | "Space" | "Derive" | "Scale" | "Guide" | "Theme" | "Layout")
        )
    }

    /// Whether the current token looks like the start of an argument
    /// (`identifier ":"`), used to distinguish a stat input from arguments.
    fn at_arg_start(&self) -> bool {
        self.at(SyntaxKind::IDENT) && self.nth_kind(1) == SyntaxKind::COLON
    }

    // --- Tree building ---

    /// Push the token at the cursor into the tree (with the given kind) and
    /// advance the raw cursor by one.
    fn push_raw(&mut self, kind: SyntaxKind) {
        let text = self.tokens[self.pos].text.clone();
        self.builder.token(kind.into(), &text);
        self.pos += 1;
    }

    /// Attach any pending trivia tokens to the current node.
    fn eat_trivia(&mut self) {
        while self.tokens[self.pos].kind.is_trivia() {
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
    }

    /// Consume the current significant token using its natural kind. Does
    /// nothing at EOF.
    fn bump(&mut self) {
        self.eat_trivia();
        if matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            return;
        }
        let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
        self.push_raw(kind);
    }

    /// Consume the current significant token but tag it with `kind` (used to
    /// retag an identifier lexeme as a contextual keyword).
    fn bump_as(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        if matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            return;
        }
        self.push_raw(kind);
    }

    /// Consume every remaining token up to EOF into an error node.
    fn drain_into_error(&mut self) {
        self.builder.start_node(SyntaxKind::ERROR.into());
        while !matches!(self.tokens[self.pos].kind, TokenKind::Eof) {
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
        self.builder.finish_node();
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    /// Consume the current significant token if it matches `kind`; otherwise
    /// record a diagnostic without consuming (recovery is the caller's job).
    fn expect(&mut self, kind: SyntaxKind, code: &'static str, message: &str) {
        if self.at(kind) {
            self.bump();
        } else {
            let span = self.current_span();
            self.error(code, message, span);
        }
    }

    // --- Program / chart (spec §7.1, §12.7) ---

    fn program(&mut self) {
        if self.at_kw("Chart") {
            self.chart_block();
        } else if self.at_eof() {
            let span = self.current_span();
            self.error("E0001", "expected Chart block", span);
        } else {
            // Search for the first `Chart`, reporting the skipped tokens.
            let span = self.current_span();
            self.error("E0001", "expected Chart block", span);
            self.builder.start_node(SyntaxKind::ERROR.into());
            while !self.at_eof() && !self.at_kw("Chart") {
                let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
                self.push_raw(kind);
            }
            self.builder.finish_node();
            if self.at_kw("Chart") {
                self.chart_block();
            }
        }

        self.eat_trivia();
        if !self.at_eof() {
            let span = self.current_span();
            self.error("E0011", "unexpected token after chart block", span);
            self.drain_into_error();
        }
    }

    fn chart_block(&mut self) {
        self.builder.start_node(SyntaxKind::CHART_BLOCK.into());
        self.bump_as(SyntaxKind::CHART_KW);
        self.expect(SyntaxKind::L_PAREN, "E0002", "expected '(' after Chart");
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
        self.expect(SyntaxKind::L_BRACE, "E0007", "expected '{'");
        self.chart_body();
        self.expect(SyntaxKind::R_BRACE, "E0008", "expected '}'");
        self.builder.finish_node();
    }

    fn chart_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.current_ident_text() {
                Some("Space") => self.space_block(),
                Some("Derive") => self.derive_decl(),
                Some("Scale") => self.decl(SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
                Some("Guide") => self.decl(SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
                Some("Theme") => self.decl(SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
                Some("Layout") => self.decl(SyntaxKind::LAYOUT_DECL, SyntaxKind::LAYOUT_KW),
                // A nested `Chart` is not allowed; stop and let trailing
                // recovery report it.
                Some("Chart") => break,
                _ => {
                    let span = self.current_span();
                    self.error("E0011", "unexpected token in chart body", span);
                    self.recover_item(/* in_space */ false);
                }
            }
            if self.pos == before {
                break;
            }
        }
    }

    // --- Space (spec §7.3, §12.8) ---

    fn space_block(&mut self) {
        self.builder.start_node(SyntaxKind::SPACE_BLOCK.into());
        self.bump_as(SyntaxKind::SPACE_KW);
        self.expect(SyntaxKind::L_PAREN, "E0002", "expected '(' after Space");
        self.algebra_expr(0); // the frame
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                break; // trailing comma
            }
            self.arg();
        }
        self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
        self.expect(SyntaxKind::L_BRACE, "E0007", "expected '{'");
        self.space_body();
        self.expect(SyntaxKind::R_BRACE, "E0008", "expected '}'");
        self.builder.finish_node();
    }

    fn space_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            // A chart-only keyword inside a space body signals a missing `}`;
            // stop so the chart body can parse it as a sibling (spec §12.17).
            if self.at_kw("Space")
                || self.at_kw("Derive")
                || self.at_kw("Layout")
                || self.at_kw("Chart")
            {
                break;
            }
            let before = self.pos;
            match self.current_ident_text() {
                Some("Scale") => self.decl(SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
                Some("Guide") => self.decl(SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
                Some("Theme") => self.decl(SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
                Some(_) if self.nth_kind(1) == SyntaxKind::L_PAREN => self.geometry_call(),
                _ => {
                    let span = self.current_span();
                    self.error("E0007", "unexpected token in space body", span);
                    self.recover_item(/* in_space */ true);
                }
            }
            if self.pos == before {
                break;
            }
        }
    }

    /// Recover from an unrecognized item by consuming tokens into an error node
    /// up to the next synchronization point (spec §12.16).
    fn recover_item(&mut self, in_space: bool) {
        self.builder.start_node(SyntaxKind::ERROR.into());
        // Always consume at least the offending token to guarantee progress.
        let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
        self.push_raw(kind);
        loop {
            if self.at_eof() || self.at(SyntaxKind::R_BRACE) || self.at_chart_keyword() {
                break;
            }
            // In a space body, any identifier may begin the next item.
            if in_space && self.at(SyntaxKind::IDENT) {
                break;
            }
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
        self.builder.finish_node();
    }

    // --- Derive (spec §7.4, §12.9) ---

    fn derive_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DERIVE_DECL.into());
        self.bump_as(SyntaxKind::DERIVE_KW);
        self.expect(SyntaxKind::IDENT, "E0010", "expected derived table name");
        self.expect(
            SyntaxKind::EQ,
            "E0016",
            "expected '=' after derived table name",
        );
        self.stat_call();
        self.builder.finish_node();
    }

    fn stat_call(&mut self) {
        if !self.at(SyntaxKind::IDENT) {
            let span = self.current_span();
            self.error("E0017", "expected stat call after '='", span);
            return;
        }
        self.builder.start_node(SyntaxKind::STAT_CALL.into());
        self.bump(); // stat name
        self.expect(SyntaxKind::L_PAREN, "E0002", "expected '(' after stat name");
        if !self.at(SyntaxKind::R_PAREN) && !self.at_eof() {
            if self.at_arg_start() {
                self.arg_list();
            } else {
                self.algebra_expr(0); // stat input
                while self.at(SyntaxKind::COMMA) {
                    self.bump();
                    if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                        break;
                    }
                    self.arg();
                }
            }
        }
        self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
        self.builder.finish_node();
    }

    // --- Calls and declarations (spec §7.5, §7.6, §12.10) ---

    fn geometry_call(&mut self) {
        self.builder.start_node(SyntaxKind::GEOMETRY_CALL.into());
        self.bump(); // geometry name
        self.expect(
            SyntaxKind::L_PAREN,
            "E0002",
            "expected '(' after geometry name",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
        self.builder.finish_node();
    }

    fn decl(&mut self, node: SyntaxKind, keyword: SyntaxKind) {
        self.builder.start_node(node.into());
        self.bump_as(keyword);
        self.expect(
            SyntaxKind::L_PAREN,
            "E0002",
            "expected '(' after declaration",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
        self.builder.finish_node();
    }

    // --- Arguments (spec §7.5, §12.11–12.12) ---

    /// Parse a comma-separated argument list up to (not including) `)`.
    fn arg_list(&mut self) {
        loop {
            if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                break;
            }
            if self.at(SyntaxKind::COMMA) {
                let span = self.current_span();
                self.error("E0014", "unexpected ','", span);
                self.bump();
                continue;
            }
            let before = self.pos;
            self.arg();
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                continue;
            }
            if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                break;
            }
            // Unexpected token after an argument: report and synchronize.
            let span = self.current_span();
            self.error("E0014", "expected ',' or ')'", span);
            self.recover_arg();
            if self.pos == before {
                break;
            }
        }
    }

    fn recover_arg(&mut self) {
        self.builder.start_node(SyntaxKind::ERROR.into());
        while !self.at_eof()
            && !self.at(SyntaxKind::COMMA)
            && !self.at(SyntaxKind::R_PAREN)
            && !self.at(SyntaxKind::R_BRACE)
            && !self.at_chart_keyword()
        {
            let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
            self.push_raw(kind);
        }
        self.builder.finish_node();
    }

    fn arg(&mut self) {
        self.builder.start_node(SyntaxKind::ARG.into());
        self.expect(SyntaxKind::IDENT, "E0010", "expected argument name");
        self.expect(
            SyntaxKind::COLON,
            "E0004",
            "expected ':' after argument name",
        );
        self.value();
        self.builder.finish_node();
    }

    // --- Values (spec §7.8, §12.13–12.14) ---

    fn value(&mut self) {
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
            SyntaxKind::L_BRACKET => self.array_value(),
            // Bare `stdin` is the stdin sentinel only when it is the whole value
            // (spec §10.1, §12.13); otherwise it is an ordinary column.
            SyntaxKind::IDENT if self.at_kw("stdin") && self.value_ends_after(1) => {
                self.builder.start_node(SyntaxKind::STDIN_VALUE.into());
                self.bump_as(SyntaxKind::STDIN_KW);
                self.builder.finish_node();
            }
            SyntaxKind::IDENT | SyntaxKind::QUOTED_IDENT | SyntaxKind::L_PAREN => {
                self.algebra_expr(0);
            }
            _ => {
                let span = self.current_span();
                self.error("E0005", "expected argument value", span);
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
    fn value_ends_after(&self, n: usize) -> bool {
        matches!(
            self.nth_kind(n),
            SyntaxKind::COMMA | SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::EOF
        )
    }

    fn value_terminator(&self) -> bool {
        matches!(
            self.nth_kind(0),
            SyntaxKind::R_PAREN
                | SyntaxKind::R_BRACE
                | SyntaxKind::R_BRACKET
                | SyntaxKind::COMMA
                | SyntaxKind::COLON
        )
    }

    fn array_value(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_VALUE.into());
        self.bump(); // '['
        loop {
            if self.at(SyntaxKind::R_BRACKET) || self.at_eof() {
                break;
            }
            if self.at(SyntaxKind::COMMA) {
                let span = self.current_span();
                self.error("E0015", "unexpected ','", span);
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
        self.expect(SyntaxKind::R_BRACKET, "E0015", "expected ',' or ']'");
        self.builder.finish_node();
    }

    // --- Algebra grammar (spec §7.7, §12.5) ---

    /// Pratt parser for algebra expressions. `min_bp` is the minimum left
    /// binding power required to continue consuming operators.
    fn algebra_expr(&mut self, min_bp: u8) {
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
    fn current_binding_power(&self) -> Option<(u8, u8)> {
        match self.current_kind() {
            TokenKind::Plus => Some((1, 2)),
            TokenKind::Star => Some((3, 4)),
            TokenKind::Slash => Some((5, 6)),
            _ => None,
        }
    }

    fn algebra_primary(&mut self) {
        match self.current_kind().clone() {
            TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => {
                self.builder.start_node(SyntaxKind::ALGEBRA_NAME.into());
                self.bump();
                self.builder.finish_node();
            }
            TokenKind::LParen => {
                self.builder.start_node(SyntaxKind::ALGEBRA_PAREN.into());
                self.bump(); // '('
                self.algebra_expr(0);
                self.expect(SyntaxKind::R_PAREN, "E0006", "expected ')'");
                self.builder.finish_node();
            }
            // A closing delimiter or EOF where a primary is expected: insert a
            // zero-width error node without consuming the delimiter (spec §12.6).
            TokenKind::RParen | TokenKind::RBrace | TokenKind::Comma | TokenKind::Eof => {
                let span = self.current_span();
                self.error("E0009", "expected algebra expression", span);
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.builder.finish_node();
            }
            // An unrelated token: consume it into an error node (spec §12.6).
            _ => {
                let span = self.current_span();
                self.error("E0009", "expected algebra expression", span);
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.bump();
                self.builder.finish_node();
            }
        }
    }
}
