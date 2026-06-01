//! Syntax kinds for the rowan concrete syntax tree (spec §11.1, §12.2).
//!
//! A single `repr(u16)` enum names every leaf token and composite node in the
//! tree. The lexer's [`TokenKind`](crate::lexer::TokenKind) maps onto the token
//! variants via [`SyntaxKind::from_token`]; the parser introduces the composite
//! node variants. Only the kinds needed by the algebra grammar are defined so
//! far — block-level node kinds are added with the block parser.

use crate::lexer::TokenKind;

/// The kind of a CST node or token.
///
/// Discriminants must stay contiguous and start at zero: [`AlgrafLang`] relies
/// on that to convert to and from rowan's raw `u16` kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // --- Trivia tokens ---
    WHITESPACE,
    COMMENT,

    // --- Literal and identifier tokens ---
    IDENT,
    QUOTED_IDENT,
    STRING,
    NUMBER,
    TRUE_KW,
    FALSE_KW,
    NULL_KW,

    // --- Punctuation tokens ---
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    L_BRACKET,
    R_BRACKET,
    COLON,
    COMMA,
    EQ,
    FAT_ARROW,
    DOT,
    STAR,
    SLASH,
    PLUS,

    // --- Contextual keyword tokens ---
    //
    // These reuse the identifier lexeme but are tagged with a distinct kind by
    // the parser so typed accessors can tell a keyword apart from a user name.
    CHART_KW,
    SPACE_KW,
    INSET_KW,
    DERIVE_KW,
    SCALE_KW,
    GUIDE_KW,
    THEME_KW,
    LAYOUT_KW,
    LET_KW,
    TABLE_KW,
    PARSE_KW,
    ALGRAF_KW,
    STDIN_KW,

    // --- Synthetic tokens ---
    /// A lexer error token (an unexpected character).
    ERROR_TOKEN,
    /// End of input. Never inserted into the tree, but reserved for clarity.
    EOF,

    // --- Composite nodes ---
    /// The tree root.
    ROOT,
    /// An identifier frame: a single plain or quoted column identifier.
    ALGEBRA_NAME,
    /// A binary algebra expression (`*` cross, `/` nest, `+` blend).
    ALGEBRA_BINARY,
    /// A parenthesized algebra expression.
    ALGEBRA_PAREN,
    /// A recovered frame-call shape retained for removed operator diagnostics.
    ALGEBRA_CALL,
    /// The root chart block.
    CHART_BLOCK,
    /// An optional top-level `Algraf(...)` source header.
    SOURCE_HEADER,
    /// A space block.
    SPACE_BLOCK,
    /// An `Inset(...) { ... }` block inside a space.
    INSET_BLOCK,
    /// A `Derive` declaration.
    DERIVE_DECL,
    /// A statistical transform call on the right of a `Derive`.
    STAT_CALL,
    /// A geometry call inside a space.
    GEOMETRY_CALL,
    /// A `Scale` declaration.
    SCALE_DECL,
    /// A `Guide` declaration.
    GUIDE_DECL,
    /// A `Theme` declaration.
    THEME_DECL,
    /// A `Layout` declaration.
    LAYOUT_DECL,
    /// A `let name = value` variable binding.
    LET_DECL,
    /// A `Table name = <source>` chart-scoped table declaration.
    TABLE_DECL,
    /// A `Parse(...)` chart-scoped temporal parse declaration.
    PARSE_DECL,
    /// A `key: value` argument.
    ARG,
    /// A literal value (string, number, boolean, or null).
    LITERAL,
    /// The bare caller-provided input sentinel in a value position.
    STDIN_VALUE,
    /// An array value.
    ARRAY_VALUE,
    /// A map value such as `["A" => "burlywood"]` (spec §7.8).
    MAP_VALUE,
    /// One `key => value` entry inside a map value.
    MAP_ENTRY,
    /// A nested call value such as `Text(size: 12)` in a property position.
    CALL_VALUE,
    /// A recovered error node.
    ERROR,

    /// Sentinel marking the highest discriminant. Never constructed.
    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    /// Map a lexer token kind to its syntax kind.
    pub fn from_token(token: &TokenKind) -> SyntaxKind {
        match token {
            TokenKind::Whitespace => SyntaxKind::WHITESPACE,
            TokenKind::Comment(_) => SyntaxKind::COMMENT,
            TokenKind::Ident(_) => SyntaxKind::IDENT,
            TokenKind::QuotedIdent(_) => SyntaxKind::QUOTED_IDENT,
            TokenKind::String(_) => SyntaxKind::STRING,
            TokenKind::Number(_) => SyntaxKind::NUMBER,
            TokenKind::True => SyntaxKind::TRUE_KW,
            TokenKind::False => SyntaxKind::FALSE_KW,
            TokenKind::Null => SyntaxKind::NULL_KW,
            TokenKind::LParen => SyntaxKind::L_PAREN,
            TokenKind::RParen => SyntaxKind::R_PAREN,
            TokenKind::LBrace => SyntaxKind::L_BRACE,
            TokenKind::RBrace => SyntaxKind::R_BRACE,
            TokenKind::LBracket => SyntaxKind::L_BRACKET,
            TokenKind::RBracket => SyntaxKind::R_BRACKET,
            TokenKind::Colon => SyntaxKind::COLON,
            TokenKind::Comma => SyntaxKind::COMMA,
            TokenKind::Equal => SyntaxKind::EQ,
            TokenKind::FatArrow => SyntaxKind::FAT_ARROW,
            TokenKind::Dot => SyntaxKind::DOT,
            TokenKind::Star => SyntaxKind::STAR,
            TokenKind::Slash => SyntaxKind::SLASH,
            TokenKind::Plus => SyntaxKind::PLUS,
            TokenKind::Error(_) => SyntaxKind::ERROR_TOKEN,
            TokenKind::Eof => SyntaxKind::EOF,
        }
    }

    /// Whether this kind is trivia (whitespace or a comment).
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// The Algraf language definition for rowan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlgrafLang {}

impl rowan::Language for AlgrafLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(
            raw.0 <= SyntaxKind::__LAST as u16,
            "raw syntax kind out of range"
        );
        // Safe: discriminants are contiguous from 0 and bounded by `__LAST`.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<AlgrafLang>;
pub type SyntaxToken = rowan::SyntaxToken<AlgrafLang>;
pub type SyntaxElement = rowan::SyntaxElement<AlgrafLang>;
