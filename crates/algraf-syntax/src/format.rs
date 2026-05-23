//! Formatter (spec §21.10).
//!
//! Produces canonical source from the CST: 4-space indentation, one block item
//! per line, spaces around algebra operators, and call arguments wrapped one per
//! line when the call would exceed the line width. Comments are preserved at the
//! item level (standalone comments on their own lines, trailing comments after
//! the item they follow); comments embedded mid-expression are not guaranteed.
//!
//! If the source has syntax errors the formatter returns it unchanged, since it
//! cannot safely reflow malformed regions (spec §2296).

use std::collections::HashMap;

use crate::ast::{
    AlgebraExpr, Arg, ChartBlock, ChartItem, Decl, DeriveDecl, LetDecl, Root, SpaceBlock,
    SpaceItem, StatCall, ValueExpr,
};
use crate::parser::parse;
use crate::syntax_kind::{SyntaxKind, SyntaxNode, SyntaxToken};

const INDENT: &str = "    ";
const MAX_WIDTH: usize = 80;

/// Format Algraf source into its canonical form.
pub fn format(source: &str) -> String {
    let parsed = parse(source);
    if !parsed.diagnostics().is_empty() {
        return source.to_string();
    }
    let root = parsed.syntax();
    let Some(chart) = Root::cast(root.clone()).and_then(|r| r.chart()) else {
        return source.to_string();
    };

    let mut printer = Printer::new(Comments::collect(&root));
    printer.chart(&chart);
    printer.out
}

/// Comments classified by the token they attach to (keyed by byte offset).
struct Comments {
    /// Standalone comments keyed by the start offset of the next code token.
    standalone: HashMap<usize, Vec<String>>,
    /// Trailing comments keyed by the start offset of the preceding code token.
    trailing: HashMap<usize, String>,
}

impl Comments {
    fn collect(root: &SyntaxNode) -> Comments {
        let tokens: Vec<SyntaxToken> = root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .collect();
        let mut standalone: HashMap<usize, Vec<String>> = HashMap::new();
        let mut trailing = HashMap::new();

        for (i, tok) in tokens.iter().enumerate() {
            if tok.kind() != SyntaxKind::COMMENT {
                continue;
            }
            let text = tok.text().trim_end().to_string();

            // A comment is "trailing" when a code token precedes it on the same
            // line (no newline in between); otherwise it stands on its own line.
            let mut newline_before = false;
            let mut prev: Option<&SyntaxToken> = None;
            for earlier in tokens[..i].iter().rev() {
                match earlier.kind() {
                    SyntaxKind::WHITESPACE => {
                        if earlier.text().contains('\n') {
                            newline_before = true;
                        }
                    }
                    SyntaxKind::COMMENT => {}
                    _ => {
                        prev = Some(earlier);
                        break;
                    }
                }
            }

            if let (Some(prev), false) = (prev, newline_before) {
                trailing.insert(offset(prev), text);
            } else {
                let key = tokens[i + 1..]
                    .iter()
                    .find(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                    .map(offset)
                    .unwrap_or(usize::MAX);
                standalone.entry(key).or_default().push(text);
            }
        }

        Comments {
            standalone,
            trailing,
        }
    }
}

struct Printer {
    out: String,
    indent: usize,
    comments: Comments,
}

impl Printer {
    fn new(comments: Comments) -> Self {
        Printer {
            out: String::new(),
            indent: 0,
            comments,
        }
    }

    // --- Low-level output ---

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Whether `text` fits within the line width at the current indent.
    fn fits(&self, text: &str) -> bool {
        self.indent * INDENT.len() + text.len() <= MAX_WIDTH
    }

    fn emit_standalone(&mut self, off: Option<usize>) {
        if let Some(off) = off {
            if let Some(comments) = self.comments.standalone.remove(&off) {
                for comment in comments {
                    self.line(&comment);
                }
            }
        }
    }

    fn append_trailing(&mut self, off: Option<usize>) {
        if let Some(off) = off {
            if let Some(comment) = self.comments.trailing.remove(&off) {
                if self.out.ends_with('\n') {
                    self.out.pop();
                }
                self.out.push_str("  ");
                self.out.push_str(&comment);
                self.out.push('\n');
            }
        }
    }

    // --- Chart ---

    fn chart(&mut self, chart: &ChartBlock) {
        let node = chart.syntax();
        self.emit_standalone(first_code(node));
        self.block_header("Chart", None, &chart.args());
        self.append_trailing(brace(node, SyntaxKind::L_BRACE));

        self.indent += 1;
        for item in chart.items() {
            self.chart_item(&item);
        }
        self.indent -= 1;

        self.emit_standalone(brace(node, SyntaxKind::R_BRACE));
        self.line("}");
        self.append_trailing(brace(node, SyntaxKind::R_BRACE));

        // Comments after the final brace, on their own line.
        self.emit_standalone(Some(usize::MAX));
    }

    fn chart_item(&mut self, item: &ChartItem) {
        match item {
            ChartItem::Space(space) => self.space(space),
            ChartItem::Derive(derive) => self.derive(derive),
            ChartItem::Let(decl) => self.let_binding(decl),
            ChartItem::Scale(decl)
            | ChartItem::Guide(decl)
            | ChartItem::Theme(decl)
            | ChartItem::Layout(decl) => self.decl(decl),
            ChartItem::Error(err) => self.raw(err.syntax()),
        }
    }

    // --- Space ---

    fn space(&mut self, space: &SpaceBlock) {
        let node = space.syntax();
        self.emit_standalone(first_code(node));
        self.block_header("Space", space.frame().as_ref(), &space.args());
        self.append_trailing(brace(node, SyntaxKind::L_BRACE));

        self.indent += 1;
        for item in space.items() {
            self.space_item(&item);
        }
        self.indent -= 1;

        self.emit_standalone(brace(node, SyntaxKind::R_BRACE));
        self.line("}");
        self.append_trailing(brace(node, SyntaxKind::R_BRACE));
    }

    fn space_item(&mut self, item: &SpaceItem) {
        match item {
            SpaceItem::Geometry(call) => {
                let name = call.name().unwrap_or_default();
                self.call_item(call.syntax(), &name, &call.args());
            }
            SpaceItem::Let(decl) => self.let_binding(decl),
            SpaceItem::Scale(decl) | SpaceItem::Guide(decl) | SpaceItem::Theme(decl) => {
                self.decl(decl)
            }
            SpaceItem::Error(err) => self.raw(err.syntax()),
        }
    }

    // --- Derive / declarations ---

    fn derive(&mut self, derive: &DeriveDecl) {
        let node = derive.syntax();
        self.emit_standalone(first_code(node));
        let name = derive.name().unwrap_or_default();
        let stat = derive.stat().map(|s| render_stat(&s)).unwrap_or_default();
        self.line(&format!("Derive {name} = {stat}"));
        self.append_trailing(last_code(node));
    }

    fn decl(&mut self, decl: &Decl) {
        let keyword = decl.keyword().to_string();
        self.call_item(decl.syntax(), &keyword, &decl.args());
    }

    fn let_binding(&mut self, decl: &LetDecl) {
        let node = decl.syntax();
        self.emit_standalone(first_code(node));
        let name = decl.name().unwrap_or_default();
        let value = decl.value().map(|v| render_value(&v)).unwrap_or_default();
        self.line(&format!("let {name} = {value}"));
        self.append_trailing(last_code(node));
    }

    // --- Call rendering ---

    /// Render a `Name(args)` call as a body item, wrapping arguments one per
    /// line when the single-line form exceeds the width.
    fn call_item(&mut self, node: &SyntaxNode, name: &str, args: &[Arg]) {
        self.emit_standalone(first_code(node));
        let inline = format!("{name}({})", inline_args(args));
        if args.len() <= 1 || self.fits(&inline) {
            self.line(&inline);
        } else {
            self.line(&format!("{name}("));
            self.indent += 1;
            for (i, arg) in args.iter().enumerate() {
                let comma = if i + 1 < args.len() { "," } else { "" };
                self.line(&format!("{}{comma}", render_arg(arg)));
            }
            self.indent -= 1;
            self.line(")");
        }
        self.append_trailing(last_code(node));
    }

    /// Render a block opener `Keyword(frame?, args) {`.
    fn block_header(&mut self, keyword: &str, frame: Option<&AlgebraExpr>, args: &[Arg]) {
        let mut inner = String::new();
        if let Some(frame) = frame {
            inner.push_str(&render_algebra(frame));
            if !args.is_empty() {
                inner.push_str(", ");
            }
        }
        inner.push_str(&inline_args(args));

        let header = format!("{keyword}({inner}) {{");
        let item_count = args.len() + usize::from(frame.is_some());
        if item_count <= 1 || self.fits(&header) {
            self.line(&header);
            return;
        }

        self.line(&format!("{keyword}("));
        self.indent += 1;
        if let Some(frame) = frame {
            let comma = if args.is_empty() { "" } else { "," };
            self.line(&format!("{}{comma}", render_algebra(frame)));
        }
        for (i, arg) in args.iter().enumerate() {
            let comma = if i + 1 < args.len() { "," } else { "" };
            self.line(&format!("{}{comma}", render_arg(arg)));
        }
        self.indent -= 1;
        self.line(") {");
    }

    /// Emit a recovered error node verbatim (should not occur for clean input).
    fn raw(&mut self, node: &SyntaxNode) {
        let text = node.text().to_string();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            self.line(trimmed);
        }
    }
}

// --- Pure rendering helpers ----------------------------------------------

fn inline_args(args: &[Arg]) -> String {
    args.iter().map(render_arg).collect::<Vec<_>>().join(", ")
}

fn render_arg(arg: &Arg) -> String {
    let key = arg.key().unwrap_or_default();
    let value = arg.value().map(|v| render_value(&v)).unwrap_or_default();
    format!("{key}: {value}")
}

fn render_value(value: &ValueExpr) -> String {
    match value {
        ValueExpr::Algebra(expr) => render_algebra(expr),
        ValueExpr::Literal(lit) => lit.text().unwrap_or_default(),
        ValueExpr::Stdin(_) => "stdin".to_string(),
        ValueExpr::Array(array) => {
            let items = array
                .values()
                .iter()
                .map(render_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{items}]")
        }
        ValueExpr::Call(call) => {
            let name = call.name().unwrap_or_default();
            format!("{name}({})", inline_args(&call.args()))
        }
        ValueExpr::Error(err) => err.syntax().text().to_string().trim().to_string(),
    }
}

fn render_algebra(expr: &AlgebraExpr) -> String {
    match expr {
        AlgebraExpr::Name(name) => name.raw_text().unwrap_or_default(),
        AlgebraExpr::Binary(binary) => {
            let op = binary.op().map(|o| o.symbol()).unwrap_or("?");
            let lhs = binary.lhs().map(|e| render_algebra(&e)).unwrap_or_default();
            let rhs = binary.rhs().map(|e| render_algebra(&e)).unwrap_or_default();
            format!("{lhs} {op} {rhs}")
        }
        AlgebraExpr::Paren(paren) => {
            let inner = paren
                .inner()
                .map(|e| render_algebra(&e))
                .unwrap_or_default();
            format!("({inner})")
        }
        AlgebraExpr::Error(err) => err.syntax().text().to_string().trim().to_string(),
    }
}

fn render_stat(stat: &StatCall) -> String {
    let name = stat.name().unwrap_or_default();
    let args = stat.args();
    let mut inner = String::new();
    if let Some(input) = stat.input() {
        inner.push_str(&render_algebra(&input));
        if !args.is_empty() {
            inner.push_str(", ");
        }
    }
    inner.push_str(&inline_args(&args));
    format!("{name}({inner})")
}

// --- Offset helpers -------------------------------------------------------

fn offset(token: &SyntaxToken) -> usize {
    u32::from(token.text_range().start()) as usize
}

fn first_code(node: &SyntaxNode) -> Option<usize> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| offset(&t))
}

fn last_code(node: &SyntaxNode) -> Option<usize> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .last()
        .map(|t| offset(&t))
}

/// The start offset of the direct child brace token of `kind`.
fn brace(node: &SyntaxNode, kind: SyntaxKind) -> Option<usize> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
        .map(|t| offset(&t))
}
