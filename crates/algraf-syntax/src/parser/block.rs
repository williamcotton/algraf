use algraf_core::codes;

use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    // --- Program / chart (spec §7.1, §12.7) ---

    /// Whether the cursor begins a chart block, including a misspelled `Chart`
    /// keyword followed by `(` or `{`.
    pub(super) fn at_chart_start(&self) -> bool {
        self.at_kw("Chart")
            || (self.at_misspelled_kw("Chart")
                && matches!(self.nth_kind(1), SyntaxKind::L_PAREN | SyntaxKind::L_BRACE))
    }

    pub(super) fn at_source_header_start(&self) -> bool {
        self.at_kw("Algraf")
            || (self.at_misspelled_kw("Algraf") && self.nth_kind(1) == SyntaxKind::L_PAREN)
    }

    pub(super) fn program(&mut self) {
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

        // A document holds one or more chart blocks, optionally interleaved
        // with document-scope declarations (spec §7.1).
        if !self.at_root_item_start() {
            let span = self.current_span();
            self.error(codes::E0001, "expected Chart block", span);
            if self.at_eof() {
                return;
            }
            // Search for the first root item, reporting the skipped tokens.
            self.builder.start_node(SyntaxKind::ERROR.into());
            while !self.at_eof() && !self.at_root_item_start() {
                let kind = SyntaxKind::from_token(&self.tokens[self.pos].kind);
                self.push_raw(kind);
            }
            self.builder.finish_node();
        }

        // Parse every top-level declaration or chart block in sequence.
        let mut saw_chart = false;
        loop {
            self.eat_trivia();
            if self.at_kw("Table") {
                self.table_decl();
                continue;
            }
            if self.at_kw("let") {
                self.let_decl();
                continue;
            }
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
            saw_chart = true;
            if self.pos == before {
                break;
            }
        }

        if !saw_chart {
            let span = self.current_span();
            self.error(codes::E0001, "expected Chart block", span);
        }

        self.eat_trivia();
        if !self.at_eof() {
            let span = self.current_span();
            self.error(codes::E0011, "unexpected token after chart block", span);
            self.drain_into_error();
        }
    }

    fn at_root_item_start(&self) -> bool {
        self.at_chart_start() || self.at_kw("Table") || self.at_kw("let")
    }

    pub(super) fn chart_block(&mut self) {
        self.builder.start_node(SyntaxKind::CHART_BLOCK.into());
        self.bump_as(SyntaxKind::CHART_KW);
        if self.at(SyntaxKind::L_PAREN) {
            self.bump();
            self.arg_list();
            self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        }
        self.expect(SyntaxKind::L_BRACE, codes::E0007, "expected '{'");
        self.chart_body();
        self.expect(SyntaxKind::R_BRACE, codes::E0008, "expected '}'");
        self.builder.finish_node();
    }

    pub(super) fn source_header(&mut self) {
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

    pub(super) fn chart_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let current = self.current_ident_text().map(str::to_string);
            match current.as_deref() {
                Some("Space") => self.space_block(),
                Some("Glyph") => self.glyph_decl(),
                Some("Derive") => self.derive_decl(),
                Some("Table") => self.table_decl(),
                Some("Parse") => self.decl(SyntaxKind::PARSE_DECL, SyntaxKind::PARSE_KW),
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

    pub(super) fn recover_misspelled_chart_item(&mut self) -> bool {
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
        if self.at_misspelled_kw("Glyph") && self.nth_kind(1) == SyntaxKind::IDENT {
            self.error(codes::E0011, "unexpected token in chart body", span);
            self.glyph_decl();
            return true;
        }

        const DECL_RECOVERY: &[(&str, SyntaxKind, SyntaxKind)] = &[
            ("Scale", SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
            ("Guide", SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
            ("Theme", SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
            ("Layout", SyntaxKind::LAYOUT_DECL, SyntaxKind::LAYOUT_KW),
            ("Parse", SyntaxKind::PARSE_DECL, SyntaxKind::PARSE_KW),
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

    pub(super) fn space_block(&mut self) {
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

    pub(super) fn space_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            // A chart-only keyword inside a space body signals a missing `}`;
            // stop so the chart body can parse it as a sibling (spec §12.17).
            if self.at_kw("Space")
                || self.at_kw("Glyph")
                || self.at_kw("Derive")
                || self.at_kw("Table")
                || self.at_kw("Parse")
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

    // --- Glyph declarations (spec §7.11, §12.8) ---

    pub(super) fn glyph_decl(&mut self) {
        self.builder.start_node(SyntaxKind::GLYPH_DECL.into());
        self.bump_as(SyntaxKind::GLYPH_KW);
        self.expect(SyntaxKind::IDENT, codes::E0010, "expected glyph name");
        self.expect(
            SyntaxKind::L_PAREN,
            codes::E0002,
            "expected '(' after glyph name",
        );
        self.arg_list();
        self.expect(SyntaxKind::R_PAREN, codes::E0006, "expected ')'");
        self.expect(SyntaxKind::L_BRACE, codes::E0007, "expected '{'");
        self.glyph_body();
        self.expect(SyntaxKind::R_BRACE, codes::E0008, "expected '}'");
        self.builder.finish_node();
    }

    pub(super) fn glyph_body(&mut self) {
        loop {
            if self.at(SyntaxKind::R_BRACE) || self.at_eof() {
                break;
            }
            if self.at_kw("Glyph")
                || self.at_kw("Derive")
                || self.at_kw("Table")
                || self.at_kw("Parse")
                || self.at_kw("Layout")
                || self.at_kw("Chart")
            {
                break;
            }
            let before = self.pos;
            match self.current_ident_text() {
                Some("Space") => self.space_block(),
                Some("let") => self.let_decl(),
                Some("Scale") => self.decl(SyntaxKind::SCALE_DECL, SyntaxKind::SCALE_KW),
                Some("Guide") => self.decl(SyntaxKind::GUIDE_DECL, SyntaxKind::GUIDE_KW),
                Some("Theme") => self.decl(SyntaxKind::THEME_DECL, SyntaxKind::THEME_KW),
                _ => {
                    let span = self.current_span();
                    self.error(codes::E0007, "unexpected token in glyph body", span);
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
    pub(super) fn recover_item(&mut self, in_space: bool) {
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

    pub(super) fn derive_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DERIVE_DECL.into());
        self.bump_as(SyntaxKind::DERIVE_KW);
        self.expect(
            SyntaxKind::IDENT,
            codes::E0010,
            "expected derived table name",
        );
        if self.at_kw("from") {
            self.bump_as(SyntaxKind::FROM_KW);
            self.expect(
                SyntaxKind::IDENT,
                codes::E0010,
                "expected input table name after `from`",
            );
        }
        self.expect(
            SyntaxKind::EQ,
            codes::E0016,
            "expected '=' after derived table name",
        );
        self.stat_call();
        self.builder.finish_node();
    }

    pub(super) fn stat_call(&mut self) {
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

    pub(super) fn let_decl(&mut self) {
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
    pub(super) fn table_decl(&mut self) {
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

    pub(super) fn geometry_call(&mut self) {
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

    pub(super) fn decl(&mut self, node: SyntaxKind, keyword: SyntaxKind) {
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
    pub(super) fn arg_list(&mut self) {
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

    pub(super) fn recover_arg(&mut self) {
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

    pub(super) fn arg(&mut self) {
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
}
