//! Lexer, parser, AST/CST, parse diagnostics, and formatter.
//!
//! See spec §6 (lexical structure), §7 (grammar), §11 (AST model), §12 (parser),
//! and §21.10 (formatting).

pub mod ast;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod syntax_kind;

pub use format::format;
pub use lexer::{tokenize, LexResult, NumberLiteral, TokenKind, TokenWithSpan};
pub use parser::{parse, parse_algebra, Parse};
pub use syntax_kind::{AlgrafLang, SyntaxKind, SyntaxNode, SyntaxToken};
