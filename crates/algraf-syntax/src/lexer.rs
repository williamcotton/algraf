//! Lexer (spec §6 lexical structure, §12.2–12.3 lexing).
//!
//! Tokenization is driven by [`logos`]. The lexer is lossless: whitespace and
//! comments are emitted as trivia tokens so the parser can build a `rowan` CST
//! that preserves formatting (spec §12.2). The token stream always ends with an
//! explicit [`TokenKind::Eof`] token (spec §12.3).
//!
//! Lexical errors are non-fatal. The lexer recovers and continues, emitting an
//! [`TokenKind::Error`] token plus a [`Diagnostic`] for: unterminated strings
//! (`E0012`), unterminated quoted identifiers (`E0019`), invalid escape
//! sequences (`E0018`), invalid number literals (`E0013`), and unexpected
//! characters (`E0011`).

use algraf_core::{Diagnostic, Span};
use logos::{Lexer, Logos};

/// A parsed numeric literal (spec §11.12).
///
/// The original lexeme text is preserved on [`TokenWithSpan::text`] for the
/// formatter (spec §11.12, §6.8).
#[derive(Debug, Clone, PartialEq)]
pub enum NumberLiteral {
    Integer(i64),
    Float(f64),
}

/// The kind of a lexical token (spec §12.3).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    QuotedIdent(String),
    String(String),
    Number(NumberLiteral),
    True,
    False,
    Null,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Equal,
    Star,
    Slash,
    Plus,
    Comment(String),
    Whitespace,
    Error(String),
    Eof,
}

impl TokenKind {
    /// Whether this token is trivia (whitespace or a comment) that the parser
    /// attaches to the CST but skips when building the typed AST (spec §12.2).
    pub fn is_trivia(&self) -> bool {
        matches!(self, TokenKind::Whitespace | TokenKind::Comment(_))
    }
}

/// A token paired with its source span and original lexeme text (spec §12.2).
#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithSpan {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

/// The result of tokenizing a source document.
#[derive(Debug, Clone)]
pub struct LexResult {
    /// Every token, including trivia, terminated by an [`TokenKind::Eof`].
    pub tokens: Vec<TokenWithSpan>,
    /// Lexical diagnostics gathered during tokenization.
    pub diagnostics: Vec<Diagnostic>,
}

/// Mutable lexer state used to accumulate diagnostics from `logos` callbacks.
#[derive(Default)]
struct LexerExtras {
    diagnostics: Vec<Diagnostic>,
}

/// Tokenize `source` into a lossless token stream plus diagnostics.
pub fn tokenize(source: &str) -> LexResult {
    let mut lexer = RawToken::lexer(source);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let range = lexer.span();
        let span = Span::new(range.start, range.end);
        let text = lexer.slice().to_string();
        match result {
            Ok(raw) => tokens.push(TokenWithSpan {
                kind: raw.into_kind(),
                span,
                text,
            }),
            Err(()) => {
                // No rule matched: an unexpected character (spec §6, E0011).
                lexer.extras.diagnostics.push(Diagnostic::error(
                    "E0011",
                    format!("unexpected character {text:?}"),
                    span,
                ));
                tokens.push(TokenWithSpan {
                    kind: TokenKind::Error(format!("unexpected character {text:?}")),
                    span,
                    text,
                });
            }
        }
    }

    let end = source.len();
    tokens.push(TokenWithSpan {
        kind: TokenKind::Eof,
        span: Span::empty(end),
        text: String::new(),
    });

    LexResult {
        tokens,
        diagnostics: lexer.extras.diagnostics,
    }
}

/// The raw token set recognized directly by `logos`.
///
/// Keyword literals (`true`, `false`, `null`) take priority over the identifier
/// regex on equal-length matches; the line-comment rule takes priority over
/// `Slash` because it is the longer match.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(extras = LexerExtras)]
enum RawToken {
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    #[regex(r"//[^\n]*", |lex| lex.slice().to_string())]
    Comment(String),

    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("null")]
    Null,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Integer, decimal, and scientific-notation numbers (spec §6.8). A leading
    // minus is part of the literal because version 0.1 has no subtraction
    // operator (spec §6.8, §6.11).
    #[regex(r"-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?", parse_number)]
    Number(NumberLiteral),

    #[token("\"", lex_string)]
    String(String),

    #[token("`", lex_quoted_ident)]
    QuotedIdent(String),

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token("=")]
    Equal,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("+")]
    Plus,
}

impl RawToken {
    fn into_kind(self) -> TokenKind {
        match self {
            RawToken::Whitespace => TokenKind::Whitespace,
            RawToken::Comment(text) => TokenKind::Comment(text),
            RawToken::True => TokenKind::True,
            RawToken::False => TokenKind::False,
            RawToken::Null => TokenKind::Null,
            RawToken::Ident(name) => TokenKind::Ident(name),
            RawToken::Number(num) => TokenKind::Number(num),
            RawToken::String(value) => TokenKind::String(value),
            RawToken::QuotedIdent(name) => TokenKind::QuotedIdent(name),
            RawToken::LParen => TokenKind::LParen,
            RawToken::RParen => TokenKind::RParen,
            RawToken::LBrace => TokenKind::LBrace,
            RawToken::RBrace => TokenKind::RBrace,
            RawToken::LBracket => TokenKind::LBracket,
            RawToken::RBracket => TokenKind::RBracket,
            RawToken::Colon => TokenKind::Colon,
            RawToken::Comma => TokenKind::Comma,
            RawToken::Equal => TokenKind::Equal,
            RawToken::Star => TokenKind::Star,
            RawToken::Slash => TokenKind::Slash,
            RawToken::Plus => TokenKind::Plus,
        }
    }
}

/// Parse a matched numeric lexeme into a [`NumberLiteral`] (spec §6.8).
fn parse_number(lex: &mut Lexer<RawToken>) -> NumberLiteral {
    let text = lex.slice();
    let is_float = text.contains('.') || text.contains('e') || text.contains('E');
    if is_float {
        match text.parse::<f64>() {
            Ok(value) => NumberLiteral::Float(value),
            Err(_) => {
                push(
                    lex,
                    Diagnostic::error("E0013", "invalid number literal", current_span(lex)),
                );
                NumberLiteral::Float(f64::NAN)
            }
        }
    } else {
        match text.parse::<i64>() {
            Ok(value) => NumberLiteral::Integer(value),
            // Integer that overflows i64: keep it as a float and flag it.
            Err(_) => match text.parse::<f64>() {
                Ok(value) => {
                    push(
                        lex,
                        Diagnostic::warning(
                            "E0013",
                            "integer literal does not fit in i64; treated as float",
                            current_span(lex),
                        ),
                    );
                    NumberLiteral::Float(value)
                }
                Err(_) => {
                    push(
                        lex,
                        Diagnostic::error("E0013", "invalid number literal", current_span(lex)),
                    );
                    NumberLiteral::Float(f64::NAN)
                }
            },
        }
    }
}

/// Lex the body of a string literal after the opening quote (spec §6.6).
fn lex_string(lex: &mut Lexer<RawToken>) -> String {
    let start = lex.span().start;
    let remainder = lex.remainder();
    let mut value = String::new();
    let mut chars = remainder.char_indices();
    let mut consumed = remainder.len();
    let mut terminated = false;

    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => {
                consumed = index + 1;
                terminated = true;
                break;
            }
            '\\' => match chars.next() {
                Some((esc_index, esc)) => {
                    if let Some(decoded) = decode_escape(esc) {
                        value.push(decoded);
                    } else {
                        // Body offset of the backslash is `index`; the escape
                        // sequence spans two bytes from the quote's body start.
                        let abs = start + 1 + index;
                        let _ = esc_index;
                        push(
                            lex,
                            Diagnostic::error(
                                "E0018",
                                format!("invalid escape sequence '\\{esc}'"),
                                Span::new(abs, abs + 1 + esc.len_utf8()),
                            ),
                        );
                        value.push(esc);
                    }
                }
                None => {
                    // Trailing backslash with no following char: unterminated.
                    break;
                }
            },
            other => value.push(other),
        }
    }

    lex.bump(consumed);

    if !terminated {
        let span = Span::new(start, lex.span().end);
        push(
            lex,
            Diagnostic::error("E0012", "unterminated string literal", span),
        );
    }

    value
}

/// Lex the body of a backtick-quoted column identifier after the opening
/// backtick (spec §6.7). Backticks inside are escaped with a backslash.
fn lex_quoted_ident(lex: &mut Lexer<RawToken>) -> String {
    let start = lex.span().start;
    let remainder = lex.remainder();
    let mut value = String::new();
    let mut chars = remainder.char_indices().peekable();
    let mut consumed = remainder.len();
    let mut terminated = false;

    while let Some((index, ch)) = chars.next() {
        match ch {
            '`' => {
                consumed = index + 1;
                terminated = true;
                break;
            }
            '\\' => match chars.peek() {
                Some(&(_, next)) if next == '`' || next == '\\' => {
                    value.push(next);
                    chars.next();
                }
                // A backslash before anything else is a literal backslash.
                _ => value.push('\\'),
            },
            other => value.push(other),
        }
    }

    lex.bump(consumed);

    if !terminated {
        let span = Span::new(start, lex.span().end);
        push(
            lex,
            Diagnostic::error("E0019", "unterminated quoted identifier", span),
        );
    }

    value
}

/// Decode a recognized escape character (spec §6.6). Returns `None` for an
/// unrecognized escape.
fn decode_escape(esc: char) -> Option<char> {
    match esc {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        '"' => Some('"'),
        '\\' => Some('\\'),
        _ => None,
    }
}

fn current_span(lex: &Lexer<RawToken>) -> Span {
    let range = lex.span();
    Span::new(range.start, range.end)
}

fn push(lex: &mut Lexer<RawToken>, diagnostic: Diagnostic) {
    lex.extras.diagnostics.push(diagnostic);
}
