use algraf_core::{Diagnostic as CoreDiagnostic, Severity};
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Url,
};

use crate::positions::span_to_range;

pub fn diagnostic_to_lsp(source: &str, uri: &Url, diagnostic: &CoreDiagnostic) -> Diagnostic {
    let related_information = diagnostic
        .related
        .iter()
        .map(|related| DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range: span_to_range(source, related.span),
            },
            message: related.message.clone(),
        })
        .collect::<Vec<_>>();

    let mut message = diagnostic.message.clone();
    if let Some(help) = &diagnostic.help {
        message.push_str("\n\nHelp: ");
        message.push_str(help);
    }

    Diagnostic {
        range: span_to_range(source, diagnostic.span),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Information => DiagnosticSeverity::INFORMATION,
            Severity::Hint => DiagnosticSeverity::HINT,
        }),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        code_description: None,
        source: Some("algraf".to_string()),
        message,
        related_information: (!related_information.is_empty()).then_some(related_information),
        tags: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_core::{codes, Span};

    fn uri() -> Url {
        Url::parse("file:///doc.ag").unwrap()
    }

    #[test]
    fn maps_severity_code_and_appends_help() {
        let source = "Chart(data: \"p.csv\") {}";
        let core = CoreDiagnostic::error(codes::E1201, "unknown geometry `Poimt`", Span::new(0, 5))
            .with_help("did you mean `Point`?");
        let lsp = diagnostic_to_lsp(source, &uri(), &core);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            lsp.code,
            Some(NumberOrString::String(codes::E1201.to_string()))
        );
        assert_eq!(lsp.source.as_deref(), Some("algraf"));
        assert!(lsp.message.contains("unknown geometry"));
        assert!(lsp.message.contains("Help: did you mean `Point`?"));
        // Span 0..5 is on the first line, columns 0..5.
        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.end.character, 5);
        assert!(lsp.related_information.is_none());
    }

    #[test]
    fn hint_severity_maps_to_hint() {
        let core = CoreDiagnostic::new(Severity::Hint, codes::H3001, "hint", Span::new(0, 1));
        let lsp = diagnostic_to_lsp("x", &uri(), &core);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::HINT));
        assert!(!lsp.message.contains("Help:"));
    }
}
