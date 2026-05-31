// --- Signature help (spec §21.15) -------------------------------------------

use algraf_semantics::registry;
use algraf_syntax::tokenize;
use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation};

use crate::completion::markup;

/// A call/array nesting frame tracked while scanning toward the cursor.
struct CallFrame {
    /// The call name (`None` for anonymous parens and array brackets).
    name: Option<String>,
    /// Whether this frame is a `(` call rather than an array `[`.
    is_call: bool,
    /// Top-level argument separators seen so far in this frame.
    commas: usize,
}

pub fn signature_help_at(text: &str, offset: usize) -> Option<SignatureHelp> {
    let prefix = &text[..offset.min(text.len())];
    let tokens: Vec<_> = tokenize(prefix)
        .tokens
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();

    let mut stack: Vec<CallFrame> = Vec::new();
    let mut previous_ident: Option<String> = None;
    for token in &tokens {
        use algraf_syntax::TokenKind;
        match &token.kind {
            TokenKind::Ident(name) => previous_ident = Some(name.clone()),
            TokenKind::LParen => {
                stack.push(CallFrame {
                    name: previous_ident.take(),
                    is_call: true,
                    commas: 0,
                });
            }
            TokenKind::LBracket => {
                stack.push(CallFrame {
                    name: None,
                    is_call: false,
                    commas: 0,
                });
                previous_ident = None;
            }
            TokenKind::RParen | TokenKind::RBracket => {
                stack.pop();
                previous_ident = None;
            }
            TokenKind::Comma => {
                if let Some(frame) = stack.last_mut() {
                    frame.commas += 1;
                }
                previous_ident = None;
            }
            _ => previous_ident = None,
        }
    }

    let frame = stack.iter().rev().find(|frame| frame.is_call)?;
    let name = frame.name.as_deref()?;
    let params = signature_params(name)?;
    Some(build_signature(name, &params, frame.commas))
}

/// The ordered parameter names for a call, drawn from the registry and the
/// declaration metadata that also drives completion (spec §13.8–13.9).
fn signature_params(name: &str) -> Option<Vec<&'static str>> {
    if let Some(geometry) = registry::geometry(name) {
        let mut params: Vec<&'static str> = geometry.prop_names().collect();
        params.push("style");
        // Declarative interactions (spec §14.25) on supported geometries.
        if registry::supports_interaction(geometry.kind) {
            params.extend(registry::INTERACTION_PROPS.iter().copied());
        }
        return Some(params);
    }
    match name {
        "Algraf" => Some(registry::declaration_arg_names(name).to_vec()),
        "Chart" => Some(registry::CHART_ARGS.to_vec()),
        "Scale" | "Guide" | "Theme" | "Layout" | "Style" | "Stop" => {
            Some(registry::declaration_arg_names(name).to_vec())
        }
        "Bin" | "Smooth" | "StepVertices" | "VectorEndpoints" | "CurveSample" | "Bin2D"
        | "HexBin" | "ContourLines" | "ContourBands" | "Density2D" | "Density2DContours"
        | "Density2DBands" | "Summary2D" | "SummaryHex" | "IntervalSegments" | "IntervalRects"
        | "IntervalMiddles" | "Simplify" | "SpatialJoin" => {
            Some(registry::declaration_arg_names(name).to_vec())
        }
        _ => None,
    }
}

fn build_signature(name: &str, params: &[&str], commas: usize) -> SignatureHelp {
    let mut label = format!("{name}(");
    let mut parameters = Vec::new();
    for (i, param) in params.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }
        let start = label.chars().map(char::len_utf16).sum::<usize>() as u32;
        label.push_str(param);
        let end = label.chars().map(char::len_utf16).sum::<usize>() as u32;
        parameters.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, end]),
            documentation: Some(markup(registry::property_doc(param))),
        });
    }
    label.push(')');

    let active_parameter = if params.is_empty() {
        None
    } else {
        Some(commas.min(params.len() - 1) as u32)
    };

    SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(parameters),
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn help(source: &str) -> SignatureHelp {
        signature_help_at(source, source.len()).expect("signature help")
    }

    #[test]
    fn geometry_signature_lists_registry_properties() {
        let sig = help("Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(");
        let info = &sig.signatures[0];
        assert!(info.label.starts_with("Point("));
        assert!(info.label.contains("fill"));
        // The first argument is active before any comma.
        assert_eq!(sig.active_parameter, Some(0));
    }

    #[test]
    fn active_parameter_advances_past_commas() {
        let sig = help("Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Point(fill: x, ");
        assert_eq!(sig.active_parameter, Some(1));
    }

    #[test]
    fn unknown_call_has_no_signature() {
        assert!(signature_help_at("Nope(", 5).is_none());
    }
}
