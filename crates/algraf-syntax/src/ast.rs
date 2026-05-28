//! Typed AST views over the rowan CST (spec section 11).
//!
//! These are lightweight wrappers around [`SyntaxNode`]s that walk CST children
//! on demand rather than owning a separate tree (spec section 11.1). Enum-shaped views
//! (`ChartItem`, `SpaceItem`, `ValueExpr`, `AlgebraExpr`) `cast` from a node by
//! inspecting its [`SyntaxKind`].

use algraf_core::Span;

use crate::source::unescape_quoted_ident;
use crate::syntax_kind::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Define a struct view over a single CST node kind.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident = $kind:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            syntax: SyntaxNode,
        }

        impl $name {
            /// Cast a syntax node to this view if it has the matching kind.
            pub fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == SyntaxKind::$kind {
                    Some(Self { syntax: node })
                } else {
                    None
                }
            }

            /// The underlying syntax node.
            pub fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

// --- Shared child helpers -------------------------------------------------

fn child_nodes<T: 'static>(node: &SyntaxNode, cast: fn(SyntaxNode) -> Option<T>) -> Vec<T> {
    node.children().filter_map(cast).collect()
}

fn first_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
}

// --- Root and chart -------------------------------------------------------

ast_node!(
    /// The tree root holding the single chart block (spec section 11.4).
    Root = ROOT
);

impl Root {
    /// The optional source header (`Algraf(...)`), if present.
    pub fn source_header(&self) -> Option<SourceHeader> {
        self.syntax.children().find_map(SourceHeader::cast)
    }

    /// The first chart block, if one was parsed.
    pub fn chart(&self) -> Option<ChartBlock> {
        self.syntax.children().find_map(ChartBlock::cast)
    }

    /// Every top-level chart block, in source order (spec section 7.1).
    pub fn charts(&self) -> Vec<ChartBlock> {
        child_nodes(&self.syntax, ChartBlock::cast)
    }
}

ast_node!(
    /// The optional top-level `Algraf(...)` source header.
    SourceHeader = SOURCE_HEADER
);

impl SourceHeader {
    /// The header's arguments (e.g. `version`, `features`).
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }
}

ast_node!(
    /// The root chart block (spec section 11.5).
    ChartBlock = CHART_BLOCK
);

impl ChartBlock {
    /// The chart's arguments (e.g. `data`, `width`).
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }

    /// The chart's body items.
    pub fn items(&self) -> Vec<ChartItem> {
        child_nodes(&self.syntax, ChartItem::cast)
    }
}

/// A chart-body item (spec section 11.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChartItem {
    Space(SpaceBlock),
    Derive(DeriveDecl),
    Table(TableDecl),
    Let(LetDecl),
    Scale(Decl),
    Guide(Decl),
    Theme(Decl),
    Layout(Decl),
    Parse(Decl),
    Error(Error),
}

impl ChartItem {
    pub fn cast(node: SyntaxNode) -> Option<ChartItem> {
        match node.kind() {
            SyntaxKind::SPACE_BLOCK => SpaceBlock::cast(node).map(ChartItem::Space),
            SyntaxKind::DERIVE_DECL => DeriveDecl::cast(node).map(ChartItem::Derive),
            SyntaxKind::TABLE_DECL => TableDecl::cast(node).map(ChartItem::Table),
            SyntaxKind::LET_DECL => LetDecl::cast(node).map(ChartItem::Let),
            SyntaxKind::SCALE_DECL => Decl::cast(node).map(ChartItem::Scale),
            SyntaxKind::GUIDE_DECL => Decl::cast(node).map(ChartItem::Guide),
            SyntaxKind::THEME_DECL => Decl::cast(node).map(ChartItem::Theme),
            SyntaxKind::LAYOUT_DECL => Decl::cast(node).map(ChartItem::Layout),
            SyntaxKind::PARSE_DECL => Decl::cast(node).map(ChartItem::Parse),
            SyntaxKind::ERROR => Error::cast(node).map(ChartItem::Error),
            _ => None,
        }
    }
}

// --- Space ----------------------------------------------------------------

ast_node!(
    /// A space block (spec section 11.6).
    SpaceBlock = SPACE_BLOCK
);

impl SpaceBlock {
    /// The algebraic frame expression.
    pub fn frame(&self) -> Option<AlgebraExpr> {
        self.syntax.children().find_map(AlgebraExpr::cast)
    }

    /// The space's arguments (e.g. `data`).
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }

    /// The space's body items.
    pub fn items(&self) -> Vec<SpaceItem> {
        child_nodes(&self.syntax, SpaceItem::cast)
    }
}

/// A space-body item (spec section 11.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceItem {
    Geometry(GeometryCall),
    Let(LetDecl),
    Scale(Decl),
    Guide(Decl),
    Theme(Decl),
    Error(Error),
}

impl SpaceItem {
    pub fn cast(node: SyntaxNode) -> Option<SpaceItem> {
        match node.kind() {
            SyntaxKind::GEOMETRY_CALL => GeometryCall::cast(node).map(SpaceItem::Geometry),
            SyntaxKind::LET_DECL => LetDecl::cast(node).map(SpaceItem::Let),
            SyntaxKind::SCALE_DECL => Decl::cast(node).map(SpaceItem::Scale),
            SyntaxKind::GUIDE_DECL => Decl::cast(node).map(SpaceItem::Guide),
            SyntaxKind::THEME_DECL => Decl::cast(node).map(SpaceItem::Theme),
            SyntaxKind::ERROR => Error::cast(node).map(SpaceItem::Error),
            _ => None,
        }
    }
}

// --- Derive / stat --------------------------------------------------------

ast_node!(
    /// A `Derive` declaration (spec section 11.7).
    DeriveDecl = DERIVE_DECL
);

impl DeriveDecl {
    /// The derived table name.
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The statistical transform on the right of `=`.
    pub fn stat(&self) -> Option<StatCall> {
        self.syntax.children().find_map(StatCall::cast)
    }
}

ast_node!(
    /// A statistical transform call (spec section 11.7).
    StatCall = STAT_CALL
);

impl StatCall {
    /// The stat name (e.g. `Bin`).
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The optional algebra input.
    pub fn input(&self) -> Option<AlgebraExpr> {
        self.syntax.children().find_map(AlgebraExpr::cast)
    }

    /// Positional algebra inputs before named arguments.
    pub fn inputs(&self) -> Vec<AlgebraExpr> {
        child_nodes(&self.syntax, AlgebraExpr::cast)
    }

    /// The stat's keyword arguments.
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }
}

// --- Let bindings ---------------------------------------------------------

ast_node!(
    /// A `let name = value` variable binding (spec sections 7.10, 11.14).
    LetDecl = LET_DECL
);

impl LetDecl {
    /// The bound variable name.
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The span of the variable-name identifier token, excluding trivia.
    pub fn name_span(&self) -> Option<Span> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| {
            let range = t.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
    }

    /// The bound value expression.
    pub fn value(&self) -> Option<ValueExpr> {
        self.syntax.children().find_map(ValueExpr::cast)
    }
}

// --- Table declarations ---------------------------------------------------

ast_node!(
    /// A `Table name = <source>` chart-scoped table declaration (spec section 7.4).
    TableDecl = TABLE_DECL
);

impl TableDecl {
    /// The declared table name.
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The span of the table-name identifier token, excluding trivia.
    pub fn name_span(&self) -> Option<Span> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| {
            let range = t.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
    }

    /// The source expression on the right of `=` (currently a string literal).
    pub fn source(&self) -> Option<ValueExpr> {
        self.syntax.children().find_map(ValueExpr::cast)
    }
}

// --- Calls and declarations ----------------------------------------------

ast_node!(
    /// A geometry call such as `Point(...)` (spec section 11.8).
    GeometryCall = GEOMETRY_CALL
);

impl GeometryCall {
    /// The geometry name (e.g. `Point`).
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The call's arguments.
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }
}

/// A `Scale` / `Guide` / `Theme` / `Layout` / `Parse` declaration (spec sections 11.5, 7.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    syntax: SyntaxNode,
}

impl Decl {
    pub fn cast(node: SyntaxNode) -> Option<Decl> {
        matches!(
            node.kind(),
            SyntaxKind::SCALE_DECL
                | SyntaxKind::GUIDE_DECL
                | SyntaxKind::THEME_DECL
                | SyntaxKind::LAYOUT_DECL
                | SyntaxKind::PARSE_DECL
        )
        .then_some(Decl { syntax: node })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }

    /// The declaration keyword as written in source (e.g. `Scale`).
    pub fn keyword(&self) -> &'static str {
        match self.syntax.kind() {
            SyntaxKind::SCALE_DECL => "Scale",
            SyntaxKind::GUIDE_DECL => "Guide",
            SyntaxKind::THEME_DECL => "Theme",
            SyntaxKind::LAYOUT_DECL => "Layout",
            SyntaxKind::PARSE_DECL => "Parse",
            _ => "",
        }
    }

    /// The declaration's arguments.
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }
}

// --- Arguments and values -------------------------------------------------

ast_node!(
    /// A `key: value` argument (spec section 11.9).
    Arg = ARG
);

impl Arg {
    /// The argument key.
    pub fn key(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The argument value.
    pub fn value(&self) -> Option<ValueExpr> {
        self.syntax.children().find_map(ValueExpr::cast)
    }
}

/// A property value (spec section 11.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    Algebra(AlgebraExpr),
    Literal(Literal),
    Stdin(StdinValue),
    Array(ArrayValue),
    Map(MapValue),
    Call(CallValue),
    Error(Error),
}

impl ValueExpr {
    pub fn cast(node: SyntaxNode) -> Option<ValueExpr> {
        match node.kind() {
            SyntaxKind::ALGEBRA_NAME | SyntaxKind::ALGEBRA_BINARY | SyntaxKind::ALGEBRA_PAREN => {
                AlgebraExpr::cast(node).map(ValueExpr::Algebra)
            }
            SyntaxKind::LITERAL => Literal::cast(node).map(ValueExpr::Literal),
            SyntaxKind::STDIN_VALUE => StdinValue::cast(node).map(ValueExpr::Stdin),
            SyntaxKind::ARRAY_VALUE => ArrayValue::cast(node).map(ValueExpr::Array),
            SyntaxKind::MAP_VALUE => MapValue::cast(node).map(ValueExpr::Map),
            SyntaxKind::CALL_VALUE => CallValue::cast(node).map(ValueExpr::Call),
            SyntaxKind::ERROR => Error::cast(node).map(ValueExpr::Error),
            _ => None,
        }
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            ValueExpr::Algebra(it) => it.syntax(),
            ValueExpr::Literal(it) => it.syntax(),
            ValueExpr::Stdin(it) => it.syntax(),
            ValueExpr::Array(it) => it.syntax(),
            ValueExpr::Map(it) => it.syntax(),
            ValueExpr::Call(it) => it.syntax(),
            ValueExpr::Error(it) => it.syntax(),
        }
    }
}

/// The kind of a literal value (spec section 11.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    String,
    Number,
    Bool,
    Null,
}

ast_node!(
    /// A literal value (spec section 11.12).
    Literal = LITERAL
);

impl Literal {
    fn token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
    }

    /// The kind of literal.
    pub fn kind(&self) -> Option<LiteralKind> {
        self.token().and_then(|t| match t.kind() {
            SyntaxKind::STRING => Some(LiteralKind::String),
            SyntaxKind::NUMBER => Some(LiteralKind::Number),
            SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => Some(LiteralKind::Bool),
            SyntaxKind::NULL_KW => Some(LiteralKind::Null),
            _ => None,
        })
    }

    /// The raw source text of the literal, including quotes for strings.
    pub fn text(&self) -> Option<String> {
        self.token().map(|t| t.text().to_string())
    }

    /// The span of the literal token, excluding preserved leading trivia.
    pub fn token_span(&self) -> Option<Span> {
        self.token().map(|token| {
            let range = token.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
    }
}

ast_node!(
    /// The bare caller-provided input sentinel in a value position
    /// (spec sections 11.10, 10.1).
    StdinValue = STDIN_VALUE
);

ast_node!(
    /// An array value (spec sections 11.10, 7.8).
    ArrayValue = ARRAY_VALUE
);

impl ArrayValue {
    /// The array elements.
    pub fn values(&self) -> Vec<ValueExpr> {
        child_nodes(&self.syntax, ValueExpr::cast)
    }
}

ast_node!(
    /// A map value such as `["A" => "burlywood"]` (spec section 7.8).
    MapValue = MAP_VALUE
);

impl MapValue {
    /// The map's entries, in source order.
    pub fn entries(&self) -> Vec<MapEntry> {
        child_nodes(&self.syntax, MapEntry::cast)
    }
}

ast_node!(
    /// One `key => value` entry inside a map value (spec section 7.8).
    MapEntry = MAP_ENTRY
);

impl MapEntry {
    /// The entry's key value (left of `=>`).
    pub fn key(&self) -> Option<ValueExpr> {
        self.syntax.children().find_map(ValueExpr::cast)
    }

    /// The entry's value (right of `=>`).
    pub fn value(&self) -> Option<ValueExpr> {
        self.syntax.children().filter_map(ValueExpr::cast).nth(1)
    }
}

ast_node!(
    /// A nested call value such as `Text(size: 12)` (spec sections 7.8, 20.8).
    CallValue = CALL_VALUE
);

impl CallValue {
    /// The call name (e.g. `Text`).
    pub fn name(&self) -> Option<String> {
        first_token(&self.syntax, SyntaxKind::IDENT).map(|t| t.text().to_string())
    }

    /// The call's arguments.
    pub fn args(&self) -> Vec<Arg> {
        child_nodes(&self.syntax, Arg::cast)
    }
}

// --- Algebra (spec section 11.11) -----------------------------------------

/// An algebra operator (spec section 11.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgebraOp {
    /// `*` Cartesian product.
    Cross,
    /// `/` nesting.
    Nest,
    /// `+` blend / union.
    Blend,
}

impl AlgebraOp {
    /// The source symbol for this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            AlgebraOp::Cross => "*",
            AlgebraOp::Nest => "/",
            AlgebraOp::Blend => "+",
        }
    }
}

/// A typed view of an algebra expression node (spec section 11.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlgebraExpr {
    Name(AlgebraName),
    Binary(AlgebraBinary),
    Paren(AlgebraParen),
    Error(Error),
}

impl AlgebraExpr {
    /// Cast a syntax node to a typed algebra expression, if it is one.
    pub fn cast(node: SyntaxNode) -> Option<AlgebraExpr> {
        match node.kind() {
            SyntaxKind::ALGEBRA_NAME => AlgebraName::cast(node).map(AlgebraExpr::Name),
            SyntaxKind::ALGEBRA_BINARY => AlgebraBinary::cast(node).map(AlgebraExpr::Binary),
            SyntaxKind::ALGEBRA_PAREN => AlgebraParen::cast(node).map(AlgebraExpr::Paren),
            SyntaxKind::ERROR => Error::cast(node).map(AlgebraExpr::Error),
            _ => None,
        }
    }

    /// The underlying syntax node.
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            AlgebraExpr::Name(it) => it.syntax(),
            AlgebraExpr::Binary(it) => it.syntax(),
            AlgebraExpr::Paren(it) => it.syntax(),
            AlgebraExpr::Error(it) => it.syntax(),
        }
    }
}

ast_node!(
    /// An identifier frame: one plain or quoted column identifier (spec section 8.2).
    AlgebraName = ALGEBRA_NAME
);

impl AlgebraName {
    fn ident_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::QUOTED_IDENT))
    }

    /// Whether the column was written with backtick syntax.
    pub fn is_quoted(&self) -> bool {
        self.ident_token()
            .is_some_and(|t| t.kind() == SyntaxKind::QUOTED_IDENT)
    }

    /// The resolved column name. Backtick-quoted names are unescaped.
    pub fn name(&self) -> Option<String> {
        self.ident_token().map(|t| match t.kind() {
            SyntaxKind::QUOTED_IDENT => unescape_quoted_ident(t.text()),
            _ => t.text().to_string(),
        })
    }

    /// The raw source lexeme, including backticks for quoted identifiers.
    pub fn raw_text(&self) -> Option<String> {
        self.ident_token().map(|t| t.text().to_string())
    }

    /// The span of the identifier token, excluding preserved leading trivia.
    pub fn ident_span(&self) -> Option<Span> {
        self.ident_token().map(|token| {
            let range = token.text_range();
            Span::new(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            )
        })
    }
}

ast_node!(
    /// A binary algebra expression (spec section 11.11).
    AlgebraBinary = ALGEBRA_BINARY
);

impl AlgebraBinary {
    /// The operator of this expression.
    pub fn op(&self) -> Option<AlgebraOp> {
        self.syntax
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find_map(|t| match t.kind() {
                SyntaxKind::STAR => Some(AlgebraOp::Cross),
                SyntaxKind::SLASH => Some(AlgebraOp::Nest),
                SyntaxKind::PLUS => Some(AlgebraOp::Blend),
                _ => None,
            })
    }

    /// The left operand.
    pub fn lhs(&self) -> Option<AlgebraExpr> {
        self.operands().next()
    }

    /// The right operand.
    pub fn rhs(&self) -> Option<AlgebraExpr> {
        self.operands().nth(1)
    }

    fn operands(&self) -> impl Iterator<Item = AlgebraExpr> {
        self.syntax.children().filter_map(AlgebraExpr::cast)
    }
}

ast_node!(
    /// A parenthesized algebra expression (spec section 11.11).
    AlgebraParen = ALGEBRA_PAREN
);

impl AlgebraParen {
    /// The inner expression between the parentheses.
    pub fn inner(&self) -> Option<AlgebraExpr> {
        self.syntax.children().find_map(AlgebraExpr::cast)
    }
}

ast_node!(
    /// A recovered error node (spec section 11.13).
    Error = ERROR
);
