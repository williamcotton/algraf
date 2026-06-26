use algraf_core::Span;
use algraf_semantics::registry;
use algraf_syntax::tokenize;
use lsp_types::{SemanticToken, SemanticTokenType, SemanticTokensLegend};

use crate::positions::span_to_range;

const SEMANTIC_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::COMMENT,
];

pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: SEMANTIC_TYPES.to_vec(),
        token_modifiers: Vec::new(),
    }
}

pub fn semantic_tokens_for(source: &str) -> Vec<SemanticToken> {
    let lexed = tokenize(source);
    let tokens = lexed.tokens;
    let mut semantic = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    let mut idx = 0usize;
    while idx < tokens.len() {
        let (token_type, span, advance) =
            if let Some((span, token_type)) = variable_reference_semantic_token(&tokens, idx) {
                (token_type, span, 2)
            } else {
                let Some(token_type) = semantic_token_type(&tokens, idx) else {
                    idx += 1;
                    continue;
                };
                (token_type, tokens[idx].span, 1)
            };
        if span.start == span.end {
            idx += advance;
            continue;
        }
        let range = span_to_range(source, span);

        // The semantic-tokens protocol forbids tokens that span multiple lines.
        // A single-line token emits once; a multi-line block comment emits one
        // token per line covering that line's portion of the comment.
        let line_count = (range.end.line - range.start.line) as usize + 1;
        let lines: Vec<&str> = source.lines().collect();
        for line_offset in 0..line_count {
            let line = range.start.line + line_offset as u32;
            let start_char = if line_offset == 0 {
                range.start.character
            } else {
                0
            };
            let end_char = if line == range.end.line {
                range.end.character
            } else {
                lines
                    .get(line as usize)
                    .map(|l| l.chars().map(char::len_utf16).sum::<usize>() as u32)
                    .unwrap_or(start_char)
            };
            let length = end_char.saturating_sub(start_char);
            if length == 0 {
                continue;
            }
            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 {
                start_char - prev_start
            } else {
                start_char
            };
            semantic.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            });
            prev_line = line;
            prev_start = start_char;
        }
        idx += advance;
    }

    semantic
}

fn variable_reference_semantic_token(
    tokens: &[algraf_syntax::TokenWithSpan],
    idx: usize,
) -> Option<(Span, u32)> {
    use algraf_syntax::TokenKind;
    let dollar = tokens.get(idx)?;
    let ident = tokens.get(idx + 1)?;
    if matches!(dollar.kind, TokenKind::Dollar)
        && matches!(ident.kind, TokenKind::Ident(_))
        && dollar.span.end == ident.span.start
    {
        Some((
            Span::new(dollar.span.start, ident.span.end),
            token_type_index(SemanticTokenType::VARIABLE),
        ))
    } else {
        None
    }
}

fn semantic_token_type(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> Option<u32> {
    use algraf_syntax::TokenKind;
    let token = &tokens[idx];
    match &token.kind {
        // The `let` keyword is a lowercase contextual keyword (spec §6.5); tag it
        // as a keyword when it begins a binding (followed by an identifier).
        TokenKind::Ident(name) if name == "let" && next_significant_is_ident(tokens, idx) => {
            Some(token_type_index(SemanticTokenType::KEYWORD))
        }
        TokenKind::Ident(name) if name == "from" && is_derive_from_keyword(tokens, idx) => {
            Some(token_type_index(SemanticTokenType::KEYWORD))
        }
        TokenKind::Ident(_) if next_significant_is_colon_all(tokens, idx) => {
            Some(token_type_index(SemanticTokenType::PROPERTY))
        }
        TokenKind::Ident(name) if declaration_name(name) || registry::geometry(name).is_some() => {
            Some(token_type_index(SemanticTokenType::FUNCTION))
        }
        TokenKind::Ident(_) | TokenKind::QuotedIdent(_) => {
            Some(token_type_index(SemanticTokenType::VARIABLE))
        }
        TokenKind::Star | TokenKind::Slash | TokenKind::Plus | TokenKind::Equal => {
            Some(token_type_index(SemanticTokenType::OPERATOR))
        }
        TokenKind::String(_) => Some(token_type_index(SemanticTokenType::STRING)),
        TokenKind::Number(_) => Some(token_type_index(SemanticTokenType::NUMBER)),
        TokenKind::True | TokenKind::False | TokenKind::Null => {
            Some(token_type_index(SemanticTokenType::KEYWORD))
        }
        TokenKind::Comment(_) => Some(token_type_index(SemanticTokenType::COMMENT)),
        _ => None,
    }
}

fn is_derive_from_keyword(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    let mut previous = tokens[..idx]
        .iter()
        .rev()
        .filter(|token| !token.kind.is_trivia());
    let Some(table_name) = previous.next() else {
        return false;
    };
    if !matches!(table_name.kind, TokenKind::Ident(_)) {
        return false;
    }
    matches!(
        previous.next().map(|token| &token.kind),
        Some(TokenKind::Ident(name)) if name == "Derive"
    )
}

fn token_type_index(token_type: SemanticTokenType) -> u32 {
    SEMANTIC_TYPES
        .iter()
        .position(|candidate| *candidate == token_type)
        .unwrap_or(0) as u32
}

fn declaration_name(name: &str) -> bool {
    matches!(
        name,
        "Algraf"
            | "Chart"
            | "Space"
            | "Glyph"
            | "Derive"
            | "Table"
            | "Parse"
            | "Scale"
            | "Guide"
            | "Theme"
            | "Layout"
            | "Style"
            | "Stop"
            | "Bin"
            | "Bin2D"
            | "HexBin"
            | "ContourLines"
            | "ContourBands"
            | "Density2D"
            | "Density2DContours"
            | "Density2DBands"
            | "Distinct"
            | "Ecdf"
            | "Qq"
            | "Summary"
            | "SummaryBin"
            | "Cut"
            | "Summary2D"
            | "SummaryHex"
            | "Smooth"
            | "StepVertices"
            | "JitterPoints"
            | "VectorEndpoints"
            | "CurveSample"
            | "IntervalSegments"
            | "IntervalRects"
            | "IntervalMiddles"
            | "Centroid"
            | "Simplify"
            | "SpatialJoin"
    )
}

fn next_significant_is_colon_all(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment(_)))
        .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
}

fn next_significant_is_ident(tokens: &[algraf_syntax::TokenWithSpan], idx: usize) -> bool {
    use algraf_syntax::TokenKind;
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment(_)))
        .is_some_and(|token| matches!(token.kind, TokenKind::Ident(_)))
}

#[cfg(test)]
mod semantic_token_tests {
    use super::{semantic_tokens_for, token_type_index, SemanticTokenType};

    #[test]
    fn multiline_block_comment_splits_per_line() {
        // The protocol forbids multi-line tokens: a block comment that spans
        // two lines must emit one COMMENT token per line (spec §6.10, §24).
        let source = "/* line one\n   line two */\nChart(data: \"d.csv\") {}";
        let tokens = semantic_tokens_for(source);
        let comment_type = token_type_index(SemanticTokenType::COMMENT);
        let comment_tokens = tokens
            .iter()
            .filter(|t| t.token_type == comment_type)
            .count();
        assert_eq!(comment_tokens, 2, "expected one comment token per line");
        // None of the emitted comment tokens may carry a multi-line length;
        // each is bounded by the absolute deltas the protocol requires.
        assert!(tokens.iter().all(|t| t.length > 0));
    }

    #[test]
    fn single_line_block_comment_is_one_token() {
        let source = "Chart(data: \"d.csv\") { /* note */ }";
        let tokens = semantic_tokens_for(source);
        let comment_type = token_type_index(SemanticTokenType::COMMENT);
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.token_type == comment_type)
                .count(),
            1
        );
    }

    #[test]
    fn non_ascii_comment_uses_utf16_token_length() {
        // Semantic-token lengths are UTF-16 units (spec §21.x). A comment with a
        // multi-byte `é` and an astral `𝄞` must report its UTF-16 length, not
        // its byte length.
        let source = "// café 𝄞\nChart(data: \"d.csv\") {}";
        let tokens = semantic_tokens_for(source);
        let comment_type = token_type_index(SemanticTokenType::COMMENT);
        let comment = tokens
            .iter()
            .find(|t| t.token_type == comment_type)
            .expect("comment token");
        // "// café 𝄞" is "// café " (8 UTF-16 units) plus `𝄞` (2 units) => 10.
        assert_eq!(comment.length, 10);
    }

    #[test]
    fn sigiled_let_reference_is_one_variable_token() {
        let source =
            "Chart(data: \"p.csv\") {\n  let c = \"#3366cc\"\n  Space(x * y) { Point(fill: $c) }\n}";
        let tokens = semantic_tokens_for(source);
        let variable_type = token_type_index(SemanticTokenType::VARIABLE);
        let target = crate::positions::offset_to_position(source, source.find("$c").unwrap());
        let mut line = 0u32;
        let mut start = 0u32;
        let mut found = false;
        for token in tokens {
            line += token.delta_line;
            if token.delta_line == 0 {
                start += token.delta_start;
            } else {
                start = token.delta_start;
            }
            if token.token_type == variable_type
                && line == target.line
                && start == target.character
                && token.length == 2
            {
                found = true;
            }
        }
        assert!(found, "expected one variable token spanning `$c`");
    }
}
