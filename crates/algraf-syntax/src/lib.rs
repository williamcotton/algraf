//! Lexer, parser, AST/CST, parse diagnostics, and formatter.
//!
//! See spec §6 (lexical structure), §7 (grammar), §11 (AST model), §12 (parser),
//! and §21.10 (formatting).

pub mod ast;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod source;
pub mod syntax_kind;

pub use format::format;
pub use lexer::{tokenize, LexResult, NumberLiteral, TokenKind, TokenWithSpan};
pub use parser::{parse, parse_algebra, Parse};
pub use source::{
    chart_data_source, chart_table_sources, document_data_source, is_source_constructor, node_span,
    source_constructor, source_constructor_path, source_expr_from_arg, source_expr_from_value,
    table_data_source, unescape_quoted_ident, unescape_string_literal, SourceConstructor,
    SourceExpr, SourceFormat,
};
pub use syntax_kind::{AlgrafLang, SyntaxKind, SyntaxNode, SyntaxToken};
