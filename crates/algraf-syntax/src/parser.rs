//! Parser (spec §12). Recursive descent for blocks and calls, Pratt parsing
//! for algebra expressions, building a lossless rowan CST plus diagnostics.
//!
//! The parser is resilient: it never panics, always advances on error, records
//! diagnostics with spans, and recovers locally so a single mistake does not
//! discard later valid blocks (spec §12.1, §12.16, §12.17).

use std::collections::HashSet;

use algraf_core::{codes, Diagnostic, DiagnosticCode, Span};
use rowan::{GreenNode, GreenNodeBuilder};

use crate::ast::{CallValue, LiteralKind, Root, ValueExpr};
use crate::lexer::{tokenize, TokenKind, TokenWithSpan};
use crate::source::{node_span, unescape_string_literal as string_value};
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

    let green = parser.builder.finish();
    let root = SyntaxNode::new_root(green.clone());
    validate_source_header(&root, &mut parser.diagnostics);
    validate_gated_source_constructors(&root, &mut parser.diagnostics);

    Parse {
        green,
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
        parser.error(
            codes::E0011,
            "unexpected token after algebra expression",
            span,
        );
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

    fn at_misspelled_kw(&self, kw: &str) -> bool {
        self.current_ident_text()
            .is_some_and(|text| text != kw && is_near_keyword(text, kw))
    }

    /// Whether the current token begins a chart-body keyword item.
    fn at_chart_keyword(&self) -> bool {
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
                    | "Algraf"
                    | "let"
            )
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

    fn error(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    /// Consume the current significant token if it matches `kind`; otherwise
    /// record a diagnostic without consuming (recovery is the caller's job).
    fn expect(&mut self, kind: SyntaxKind, code: DiagnosticCode, message: &str) {
        if self.at(kind) {
            self.bump();
        } else {
            let span = self.current_span();
            self.error(code, message, span);
        }
    }

    // --- Program / chart (spec §7.1, §12.7) ---

    /// Whether the cursor begins a chart block, including a misspelled `Chart`
    /// keyword followed by `(`.
    fn at_chart_start(&self) -> bool {
        self.at_kw("Chart")
            || (self.at_misspelled_kw("Chart") && self.nth_kind(1) == SyntaxKind::L_PAREN)
    }

    fn at_source_header_start(&self) -> bool {
        self.at_kw("Algraf")
            || (self.at_misspelled_kw("Algraf") && self.nth_kind(1) == SyntaxKind::L_PAREN)
    }

    fn program(&mut self) {
        let mut saw_header = false;
        while self.at_source_header_start() {
            if saw_header {
                let span = self.current_span();
                self.error(codes::E0022, "duplicate Algraf source header", span);
            }
            if !self.at_kw("Algraf") {
                let span = self.current_span();
                self.error(codes::E0022, "expected Algraf source header", span);
            }
            self.source_header();
            saw_header = true;
            self.eat_trivia();
        }

        // A document holds one or more chart blocks (spec §7.1). The first
        // block is required; later blocks render independently.
        if !self.at_chart_start() {
            let span = self.current_span();
            self.error(codes::E0001, "expected Chart block", span);
            if self.at_eof() {
                return;
            }
            // Search for the first `Chart`, reporting the skipped tokens.
            self.builder.start_node(SyntaxKind::ERROR.into());
            while !self.at_eof() && !self.at_kw("Chart") {
                let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
                self.push_raw(kind);
            }
            self.builder.finish_node();
        }

        // Parse every chart block in sequence.
        loop {
            self.eat_trivia();
            if !self.at_chart_start() {
                break;
            }
            // A misspelled keyword still parses as a chart but is flagged.
            if !self.at_kw("Chart") {
                let span = self.current_span();
                self.error(codes::E0001, "expected Chart block", span);
            }
            let before = self.pos;
            self.chart_block();
            if self.pos == before {
                break;
            }
        }

        self.eat_trivia();
        if !self.at_eof() {
            let span = self.current_span();
            self.error(codes::E0011, "unexpected token after chart block", span);
            self.drain_into_error();
        }
    }

    fn chart_block(&mut self) {
        self.builder.start_node(SyntaxKind::CHART_BLOCK.into());
        self.bump_as(SyntaxKind::CHART_KW);
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after Chart",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.expect(SyntaxKind::L_BRACE, codes::E0007, "expected '{'");
        self.chart_body();
        self.expect(SyntaxKind::R_BRACE, codes::E0008, "expected '}'");
        self.builder.finish_node();
    }

    fn source_header(&mut self) {
        self.builder.start_node(SyntaxKind::SOURCE_HEADER.into());
        self.bump_as(SyntaxKind::ALGRAF_KW);
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after Algraf",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.builder.finish_node();
    }

    fn chart_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let current = self.current_ident_text().map(str::to_string);
            match current.as_deref() {
                Some("Space") => self.space_block(),
                Some("Derive") => self.derive_decl(),
                Some("Table") => self.table_decl(),
                Some("let") => self.let_decl(),
                Some("Scale") => self.decl(SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
                Some("Guide") => self.decl(SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
                Some("Theme") => self.decl(SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
                Some("Layout") => self.decl(SyntaxKind::LAYOUT_DECL, SyntaxKind::LAYOUT_KW),
                // A nested `Chart` is not allowed; stop and let trailing
                // recovery report it.
                Some("Chart") => break,
                _ if self.recover_misspelled_chart_item() => {}
                _ => {
                    let span = self.current_span();
                    self.error(codes::E0011, "unexpected token in chart body", span);
                    self.recover_item(/* in_space */ false);
                }
            }
            if self.pos == before {
                break;
            }
        }
    }

    fn recover_misspelled_chart_item(&mut self) -> bool {
        if self.current_ident_text().is_none() {
            return false;
        }
        let span = self.current_span();

        if self.at_misspelled_kw("Space") && self.nth_kind(1) == SyntaxKind::L_PAREN {
            self.error(codes::E0011, "unexpected token in chart body", span);
            self.space_block();
            return true;
        }
        if self.at_misspelled_kw("Derive") {
            self.error(codes::E0011, "unexpected token in chart body", span);
            self.derive_decl();
            return true;
        }

        const DECL_RECOVERY: &[(&str, SyntaxKind, SyntaxKind)] = &[
            ("Scale", SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
            ("Guide", SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
            ("Theme", SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
            ("Layout", SyntaxKind::LAYOUT_DECL, SyntaxKind::LAYOUT_KW),
        ];
        for (name, node, keyword) in DECL_RECOVERY {
            if self.at_misspelled_kw(name) && self.nth_kind(1) == SyntaxKind::L_PAREN {
                self.error(codes::E0011, "unexpected token in chart body", span);
                self.decl(*node, *keyword);
                return true;
            }
        }

        false
    }

    // --- Space (spec §7.3, §12.8) ---

    fn space_block(&mut self) {
        self.builder.start_node(SyntaxKind::SPACE_BLOCK.into());
        self.bump_as(SyntaxKind::SPACE_KW);
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after Space",
        );
        self.algebra_expr(0); // the frame
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                break; // trailing comma
            }
            self.arg();
        }
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.expect(SyntaxKind::L_BRACE, codes::E0007, "expected '{'");
        self.space_body();
        self.expect(SyntaxKind::R_BRACE, codes::E0008, "expected '}'");
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
                || self.at_kw("Table")
                || self.at_kw("Layout")
                || self.at_kw("Chart")
            {
                break;
            }
            let before = self.pos;
            match self.current_ident_text() {
                Some("let") => self.let_decl(),
                Some("Scale") => self.decl(SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
                Some("Guide") => self.decl(SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
                Some("Theme") => self.decl(SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
                Some(_) if self.nth_kind(1) == SyntaxKind::L_PAREN => self.geometry_call(),
                _ => {
                    let span = self.current_span();
                    self.error(codes::E0007, "unexpected token in space body", span);
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
        self.expect(
            SyntaxKind::IDENT,
            codes::E0010,
            "expected derived table name",
        );
        self.expect(
            SyntaxKind::EQ,
            codes::E0016,
            "expected '=' after derived table name",
        );
        self.stat_call();
        self.builder.finish_node();
    }

    fn stat_call(&mut self) {
        if !self.at(SyntaxKind::IDENT) {
            let span = self.current_span();
            self.error(codes::E0017, "expected stat call after '='", span);
            return;
        }
        self.builder.start_node(SyntaxKind::STAT_CALL.into());
        self.bump(); // stat name
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after stat name",
        );
        if !self.at(SyntaxKind::R_PAREN) && !self.at_eof() {
            if self.at_arg_start() {
                self.arg_list();
            } else {
                self.algebra_expr(0); // first stat input
                while self.at(SyntaxKind::COMMA) {
                    self.bump();
                    if self.at(SyntaxKind::R_PAREN) || self.at_eof() {
                        break;
                    }
                    if self.at_arg_start() {
                        self.arg();
                    } else {
                        self.algebra_expr(0);
                    }
                }
            }
        }
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.builder.finish_node();
    }

    // --- Let bindings (spec §7.10, §12.9) ---

    fn let_decl(&mut self) {
        self.builder.start_node(SyntaxKind::LET_DECL.into());
        self.bump_as(SyntaxKind::LET_KW);
        self.expect(
            SyntaxKind::IDENT,
            codes::E0010,
            "expected variable name after `let`",
        );
        self.expect(SyntaxKind::EQ, codes::E0021, "expected '=' in let binding");
        self.value();
        self.builder.finish_node();
    }

    // --- Table declarations (spec §7.4, §10.x) ---

    /// Parse a `Table name = <source>` chart-scoped declaration. The source is
    /// currently a string-literal CSV path; the value position is left open for
    /// v0.7 source constructors (spec §10.x).
    fn table_decl(&mut self) {
        self.builder.start_node(SyntaxKind::TABLE_DECL.into());
        self.bump_as(SyntaxKind::TABLE_KW);
        self.expect(SyntaxKind::IDENT, codes::E0010, "expected table name");
        self.expect(
            SyntaxKind::EQ,
            codes::E0016,
            "expected '=' after table name",
        );
        self.value();
        self.builder.finish_node();
    }

    // --- Calls and declarations (spec §7.5, §7.6, §12.10) ---

    fn geometry_call(&mut self) {
        self.builder.start_node(SyntaxKind::GEOMETRY_CALL.into());
        self.bump(); // geometry name
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after geometry name",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.builder.finish_node();
    }

    fn decl(&mut self, node: SyntaxKind, keyword: SyntaxKind) {
        self.builder.start_node(node.into());
        self.bump_as(keyword);
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after declaration",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
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
                self.error(codes::E0014, "unexpected ','", span);
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
            self.error(codes::E0014, "expected ',' or ')'", span);
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
        // An argument is either keyed (`name: value`) or positional (a bare
        // value, e.g. the path in `GeoJson("file.geojson")`, spec §10.11). A
        // leading `IDENT :` is the key; anything else is parsed as a value.
        if self.at(SyntaxKind::IDENT) && self.nth_kind(1) == SyntaxKind::COLON {
            self.bump(); // argument name
            self.bump(); // ':'
            self.value();
        } else {
            self.value();
        }
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
            SyntaxKind::L_BRACKET => self.bracket_value(),
            // Bare `stdin` is the stdin sentinel only when it is the whole value
            // (spec §10.1, §12.13); otherwise it is an ordinary column.
            SyntaxKind::IDENT if self.at_kw("stdin") && self.value_ends_after(1) => {
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

    /// Parse a nested call value `Name(args)` (spec §7.8, §20.8).
    fn call_value(&mut self) {
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
    fn bracket_value(&mut self) {
        if self.bracket_is_map() {
            self.map_value();
        } else {
            self.array_value();
        }
    }

    /// Whether the bracket beginning at the cursor contains a top-level `=>`,
    /// marking it as a map literal rather than an array.
    fn bracket_is_map(&self) -> bool {
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

    fn array_value(&mut self) {
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
    fn map_value(&mut self) {
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

    fn map_entry(&mut self) {
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

fn is_near_keyword(text: &str, keyword: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    let Some(keyword_first) = keyword.chars().next() else {
        return false;
    };
    first.eq_ignore_ascii_case(&keyword_first) && edit_distance_ascii(text, keyword) <= 2
}

fn edit_distance_ascii(a: &str, b: &str) -> usize {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

const KNOWN_FEATURE_GATES: &[&str] = &["sql", "network", "plugins", "experimental"];

fn validate_source_header(root: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    let Some(root) = Root::cast(root.clone()) else {
        return;
    };
    let Some(header) = root.source_header() else {
        return;
    };

    let mut seen_args = HashSet::new();
    let mut saw_version = false;
    for arg in header.args() {
        let arg_span = node_span(arg.syntax());
        let Some(key) = arg.key() else {
            diagnostics.push(Diagnostic::error(
                codes::E0022,
                "Algraf source header arguments must be named",
                arg_span,
            ));
            continue;
        };
        if !seen_args.insert(key.clone()) {
            diagnostics.push(Diagnostic::error(
                codes::E0022,
                format!("duplicate Algraf source header argument `{key}`"),
                arg_span,
            ));
            continue;
        }

        match key.as_str() {
            "version" => {
                saw_version = true;
                validate_header_version(&arg, diagnostics);
            }
            "features" => validate_header_features(&arg, diagnostics),
            _ => diagnostics.push(Diagnostic::error(
                codes::E0022,
                format!("unsupported Algraf source header argument `{key}`"),
                arg_span,
            )),
        }
    }

    if !saw_version {
        diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`Algraf(...)` requires `version: \"0.21\"`",
            node_span(header.syntax()),
        ));
    }
}

fn validate_header_version(arg: &crate::ast::Arg, diagnostics: &mut Vec<Diagnostic>) {
    match arg.value() {
        Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
            let raw = string_value(&lit.text().unwrap_or_default());
            let span = node_span(lit.syntax());
            match parse_language_version(&raw) {
                Some((major, minor, _patch)) if major == 0 && minor <= 21 => {}
                Some(_) => diagnostics.push(Diagnostic::error(
                    codes::E0023,
                    format!("unsupported Algraf language version `{raw}`"),
                    span,
                )),
                None => diagnostics.push(Diagnostic::error(
                    codes::E0022,
                    "`version` expects a string like \"0.21\"",
                    span,
                )),
            }
        }
        Some(value) => diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`version` expects a string literal",
            node_span(value.syntax()),
        )),
        None => diagnostics.push(Diagnostic::error(
            codes::E0022,
            "`version` expects a string literal",
            node_span(arg.syntax()),
        )),
    }
}

fn parse_language_version(raw: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = raw.split('.').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = if parts.len() == 3 {
        parts[2].parse().ok()?
    } else {
        0
    };
    Some((major, minor, patch))
}

fn validate_header_features(arg: &crate::ast::Arg, diagnostics: &mut Vec<Diagnostic>) {
    let Some(value) = arg.value() else {
        diagnostics.push(Diagnostic::error(
            codes::E1703,
            "`features` expects an array of string feature gates",
            node_span(arg.syntax()),
        ));
        return;
    };
    let ValueExpr::Array(array) = value else {
        diagnostics.push(Diagnostic::error(
            codes::E1703,
            "`features` expects an array of string feature gates",
            node_span(value.syntax()),
        ));
        return;
    };

    let mut seen = HashSet::new();
    for item in array.values() {
        let span = node_span(item.syntax());
        let Some(gate) = string_literal_value(&item) else {
            diagnostics.push(Diagnostic::error(
                codes::E1703,
                "`features` entries must be string literals",
                span,
            ));
            continue;
        };
        if !KNOWN_FEATURE_GATES.contains(&gate.as_str()) {
            diagnostics.push(Diagnostic::error(
                codes::E0024,
                format!("unknown feature gate `{gate}`"),
                span,
            ));
            continue;
        }
        if !seen.insert(gate.clone()) {
            diagnostics.push(Diagnostic::error(
                codes::E0024,
                format!("duplicate feature gate `{gate}`"),
                span,
            ));
        }
    }
}

fn validate_gated_source_constructors(root: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    let sql_enabled = source_version_at_least(root, 21) && source_has_feature(root, "sql");
    for call in root.descendants().filter_map(CallValue::cast) {
        if call.name().as_deref() == Some("Sqlite") && !sql_enabled {
            diagnostics.push(
                Diagnostic::error(
                    codes::E0025,
                    "`Sqlite(...)` requires Algraf version 0.21 and the `sql` feature gate",
                    node_span(call.syntax()),
                )
                .with_help(r#"add `Algraf(version: "0.21", features: ["sql"])` before the chart"#),
            );
        }
    }
}

fn source_version_at_least(root: &SyntaxNode, minor: u64) -> bool {
    source_header_arg(root, "version")
        .and_then(|arg| match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                let raw = string_value(&lit.text().unwrap_or_default());
                parse_language_version(&raw)
            }
            _ => None,
        })
        .is_some_and(|(major, declared_minor, _patch)| major == 0 && declared_minor >= minor)
}

fn source_has_feature(root: &SyntaxNode, feature: &str) -> bool {
    let Some(arg) = source_header_arg(root, "features") else {
        return false;
    };
    let Some(ValueExpr::Array(array)) = arg.value() else {
        return false;
    };
    array
        .values()
        .iter()
        .any(|item| string_literal_value(item).as_deref() == Some(feature))
}

fn source_header_arg(root: &SyntaxNode, key: &str) -> Option<crate::ast::Arg> {
    Root::cast(root.clone())?
        .source_header()?
        .args()
        .into_iter()
        .find(|arg| arg.key().as_deref() == Some(key))
}

fn string_literal_value(value: &ValueExpr) -> Option<String> {
    match value {
        ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
            Some(string_value(&lit.text().unwrap_or_default()))
        }
        _ => None,
    }
}
