//! Parser (spec §12). Recursive descent for blocks and calls, Pratt parsing
//! for algebra expressions, building a lossless rowan CST plus diagnostics.
//!
//! The parser is resilient: it never panics, always advances on error, records
//! diagnostics with spans, and recovers locally so a single mistake does not
//! discard later valid blocks (spec §12.1, §12.16, §12.17).

use algraf_core::{closest, codes, Diagnostic};
use rowan::{GreenNode, GreenNodeBuilder};

use crate::lexer::{tokenize, TokenWithSpan};
use crate::syntax_kind::{SyntaxKind, SyntaxNode};

mod algebra;
mod block;
mod cursor;
mod tree;
mod validator;
mod value;

use validator::{validate_gated_source_constructors, validate_source_header};

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
}

fn is_near_keyword(text: &str, keyword: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    let Some(keyword_first) = keyword.chars().next() else {
        return false;
    };
    first.eq_ignore_ascii_case(&keyword_first) && closest(text, std::iter::once(keyword)).is_some()
}
