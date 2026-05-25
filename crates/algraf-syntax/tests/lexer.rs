//! Lexer tests (spec §27.2).

use algraf_core::Severity;
use algraf_syntax::lexer::{tokenize, NumberLiteral, TokenKind};

/// Collect non-trivia, non-EOF token kinds for concise assertions.
fn significant(source: &str) -> Vec<TokenKind> {
    tokenize(source)
        .tokens
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivia() && *k != TokenKind::Eof)
        .collect()
}

#[test]
fn test_identifiers() {
    assert_eq!(
        significant("flipper_length body_mass _x x9"),
        vec![
            TokenKind::Ident("flipper_length".into()),
            TokenKind::Ident("body_mass".into()),
            TokenKind::Ident("_x".into()),
            TokenKind::Ident("x9".into()),
        ]
    );
}

#[test]
fn test_keywords_are_not_identifiers() {
    assert_eq!(
        significant("true false null"),
        vec![TokenKind::True, TokenKind::False, TokenKind::Null]
    );
    // A keyword prefix is still an ordinary identifier.
    assert_eq!(
        significant("truely"),
        vec![TokenKind::Ident("truely".into())]
    );
}

#[test]
fn test_quoted_identifiers() {
    assert_eq!(
        significant("`flipper length` `body mass (g)`"),
        vec![
            TokenKind::QuotedIdent("flipper length".into()),
            TokenKind::QuotedIdent("body mass (g)".into()),
        ]
    );
}

#[test]
fn test_quoted_identifier_escapes_backtick() {
    assert_eq!(
        significant(r"`a\`b`"),
        vec![TokenKind::QuotedIdent("a`b".into())]
    );
}

#[test]
fn test_strings() {
    assert_eq!(
        significant(r#""steelblue" "#),
        vec![TokenKind::String("steelblue".into())]
    );
}

#[test]
fn test_escaped_strings() {
    assert_eq!(
        significant(r#""line\nbreak\ttab\"quote\\slash""#),
        vec![TokenKind::String("line\nbreak\ttab\"quote\\slash".into())]
    );
}

#[test]
fn test_unicode_escapes_in_strings_and_quoted_identifiers() {
    assert_eq!(
        significant(r#""Revenue \u{2014}" `city\u{20}name`"#),
        vec![
            TokenKind::String("Revenue \u{2014}".into()),
            TokenKind::QuotedIdent("city name".into()),
        ]
    );
}

#[test]
fn test_invalid_unicode_escape_reports_diagnostic() {
    let result = tokenize(r#""bad \u{110000}""#);
    assert!(result.diagnostics.iter().any(|d| d.code == "E0018"));
}

#[test]
fn test_string_is_distinct_from_quoted_identifier() {
    // Double quotes are always string literals; backticks are column ids.
    assert_eq!(
        significant(r#""species" `species`"#),
        vec![
            TokenKind::String("species".into()),
            TokenKind::QuotedIdent("species".into()),
        ]
    );
}

#[test]
fn test_numbers() {
    assert_eq!(
        significant("0 25 1000 0.5 2.71 1e3 2.5e-4"),
        vec![
            TokenKind::Number(NumberLiteral::Integer(0)),
            TokenKind::Number(NumberLiteral::Integer(25)),
            TokenKind::Number(NumberLiteral::Integer(1000)),
            TokenKind::Number(NumberLiteral::Float(0.5)),
            TokenKind::Number(NumberLiteral::Float(2.71)),
            TokenKind::Number(NumberLiteral::Float(1e3)),
            TokenKind::Number(NumberLiteral::Float(2.5e-4)),
        ]
    );
}

#[test]
fn test_negative_number_lexes_as_signed() {
    assert_eq!(
        significant("-10.2"),
        vec![TokenKind::Number(NumberLiteral::Float(-10.2))]
    );
}

#[test]
fn test_punctuation() {
    assert_eq!(
        significant("(){}[]:,=*/+"),
        vec![
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Colon,
            TokenKind::Comma,
            TokenKind::Equal,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Plus,
        ]
    );
}

#[test]
fn test_comments() {
    let result = tokenize("Point() // trailing comment\n");
    let comment = result
        .tokens
        .iter()
        .find(|t| matches!(t.kind, TokenKind::Comment(_)))
        .expect("expected a comment token");
    assert_eq!(
        comment.kind,
        TokenKind::Comment("// trailing comment".into())
    );
    // Slash is the comment opener, not a division operator.
    assert!(!significant("// a comment").contains(&TokenKind::Slash));
}

#[test]
fn test_block_comment() {
    // A `/* ... */` block comment is a single Comment trivia token (spec §6.10).
    let result = tokenize("Point(/* inline */) ");
    let comment = result
        .tokens
        .iter()
        .find(|t| matches!(t.kind, TokenKind::Comment(_)))
        .expect("expected a comment token");
    assert_eq!(comment.kind, TokenKind::Comment("/* inline */".into()));
    assert!(result.diagnostics.is_empty());
    // `/*` is the comment opener, not a division operator.
    assert!(!significant("/* c */").contains(&TokenKind::Slash));
}

#[test]
fn test_multiline_block_comment() {
    // Block comments may span lines and stop at the first `*/` (non-nested).
    let src = "/* line one\n line two */\nPoint()";
    let result = tokenize(src);
    let comment = result
        .tokens
        .iter()
        .find(|t| matches!(t.kind, TokenKind::Comment(_)))
        .expect("expected a comment token");
    assert_eq!(
        comment.kind,
        TokenKind::Comment("/* line one\n line two */".into())
    );
    // The first `*/` closes: nesting is not supported.
    let nested = tokenize("/* a /* b */ c */");
    let comments: Vec<_> = nested
        .tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Comment(_)))
        .collect();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, TokenKind::Comment("/* a /* b */".into()));
}

#[test]
fn test_unterminated_block_comment() {
    // An unterminated block comment runs to EOF and emits E0020 (spec §6.10).
    let result = tokenize("Point() /* never closed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E0020"),
        "expected E0020 for unterminated block comment"
    );
}

#[test]
fn test_trivia_is_preserved() {
    // Whitespace and comments are retained for the lossless CST (spec §12.2).
    let kinds: Vec<_> = tokenize("a // c\nb")
        .tokens
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Whitespace,
            TokenKind::Comment("// c".into()),
            TokenKind::Whitespace,
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_always_ends_with_eof() {
    let tokens = tokenize("").tokens;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::Eof);
    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 0);
}

#[test]
fn test_invalid_character_recovers() {
    let result = tokenize("a @ b");
    // The '@' produces an Error token but lexing continues.
    assert!(result
        .tokens
        .iter()
        .any(|t| matches!(t.kind, TokenKind::Error(_))));
    assert!(result.diagnostics.iter().any(|d| d.code == "E0011"));
    assert_eq!(
        significant("a @ b")
            .into_iter()
            .filter(|k| matches!(k, TokenKind::Ident(_)))
            .count(),
        2
    );
}

#[test]
fn test_unterminated_string_diagnostic() {
    let result = tokenize(r#""no closing quote"#);
    let diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == "E0012")
        .expect("expected unterminated string diagnostic");
    assert_eq!(diag.severity, Severity::Error);
    // The token still carries the scanned value for recovery.
    assert_eq!(
        result.tokens[0].kind,
        TokenKind::String("no closing quote".into())
    );
}

#[test]
fn test_unterminated_quoted_identifier_diagnostic() {
    let result = tokenize("`unclosed");
    assert!(result.diagnostics.iter().any(|d| d.code == "E0019"));
}

#[test]
fn test_invalid_escape_diagnostic() {
    let result = tokenize(r#""bad \q escape""#);
    assert!(result.diagnostics.iter().any(|d| d.code == "E0018"));
    // The string still lexes, keeping the offending char.
    assert_eq!(
        result.tokens[0].kind,
        TokenKind::String("bad q escape".into())
    );
}

#[test]
fn test_spans_are_byte_offsets() {
    let result = tokenize("Point(fill: species)");
    let point = &result.tokens[0];
    assert_eq!(point.kind, TokenKind::Ident("Point".into()));
    assert_eq!(point.span.start, 0);
    assert_eq!(point.span.end, 5);
    assert_eq!(
        &"Point(fill: species)"[point.span.start..point.span.end],
        "Point"
    );
}

#[test]
fn test_spans_with_non_ascii() {
    // Spans MUST be byte offsets even with multi-byte characters (spec §6.1).
    let source = "`naïve` °";
    let result = tokenize(source);
    let ident = &result.tokens[0];
    assert_eq!(ident.kind, TokenKind::QuotedIdent("naïve".into()));
    // `naïve` contains a 2-byte 'ï', so the closing backtick is at byte 7.
    assert_eq!(ident.span.start, 0);
    assert_eq!(ident.span.end, 8);
    assert_eq!(&source[ident.span.start..ident.span.end], "`naïve`");
}

#[test]
fn test_minimal_chart_token_stream() {
    let source = "Chart(data: \"penguins.csv\") {}";
    assert_eq!(
        significant(source),
        vec![
            TokenKind::Ident("Chart".into()),
            TokenKind::LParen,
            TokenKind::Ident("data".into()),
            TokenKind::Colon,
            TokenKind::String("penguins.csv".into()),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]
    );
}
