//! Lossless CST JSON serialization shared by CLI and browser/WASM APIs.

use serde_json::{json, Value};

use crate::SyntaxNode;

/// Convert a syntax node and its tokens to the stable JSON tree shape used by
/// `algraf ast --json`.
pub fn node_to_json(node: &SyntaxNode) -> Value {
    let range = node.text_range();
    let mut children = Vec::new();
    for element in node.children_with_tokens() {
        if let Some(child) = element.as_node() {
            children.push(node_to_json(child));
        } else if let Some(token) = element.as_token() {
            let trange = token.text_range();
            children.push(json!({
                "token": format!("{:?}", token.kind()),
                "text": token.text(),
                "span": { "start": usize::from(trange.start()), "end": usize::from(trange.end()) },
            }));
        }
    }
    json!({
        "node": format!("{:?}", node.kind()),
        "span": { "start": usize::from(range.start()), "end": usize::from(range.end()) },
        "children": children,
    })
}
