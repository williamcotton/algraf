//! Diagnostics (spec §12.15, §13.16).
//!
//! A diagnostic is a machine-readable error or warning carrying a stable code,
//! a severity, a message, a primary source span, optional related spans, and
//! optional help text.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::span::Span;

/// A registered, stable diagnostic code (spec §26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagnosticCode(&'static str);

impl DiagnosticCode {
    /// Create a code constant. All public values should be declared in
    /// [`codes`] and included in [`all_codes`].
    pub const fn new(code: &'static str) -> Self {
        DiagnosticCode(code)
    }

    /// The wire/code string, such as `E1101`.
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// Look up a registered diagnostic code by its wire string.
    pub fn parse(code: &str) -> Option<Self> {
        all_codes()
            .iter()
            .copied()
            .find(|registered| registered.as_str() == code)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl PartialEq<&str> for DiagnosticCode {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<DiagnosticCode> for &str {
    fn eq(&self, other: &DiagnosticCode) -> bool {
        *self == other.0
    }
}

impl Serialize for DiagnosticCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        DiagnosticCode::parse(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown diagnostic code `{code}`")))
    }
}

macro_rules! register_codes {
    ($($name:ident),+ $(,)?) => {
        /// Canonical diagnostic-code constants (spec §26).
        pub mod codes {
            use super::DiagnosticCode;

            $(
                pub const $name: DiagnosticCode = DiagnosticCode::new(stringify!($name));
            )+
        }

        const REGISTERED_CODES: &[DiagnosticCode] = &[
            $(codes::$name,)+
        ];
    };
}

register_codes! {
    E0001, E0002, E0003, E0004, E0005, E0006, E0007, E0008,
    E0009, E0010, E0011, E0012, E0013, E0014, E0015, E0016,
    E0017, E0018, E0019, E0020, E0021, E0022, E0023, E0024,
    E0025, E1001, E1002, E1003, E1004, E1005, E1006, E1007,
    E1008, E1009, E1010, E1011, E1012, E1013, E1014, E1015,
    E1016, E1017, E1018, E1019, E1020, E1021, E1022, E1101, E1102, E1103, E1104,
    E1105, E1106, E1107, E1108, E1201, E1202, E1203, E1204,
    E1205, E1206, E1207, E1301, E1302, E1303, E1304, E1305,
    E1306, E1401, E1402, E1403, E1404, E1405, E1406, E1407,
    E1408, E1501, E1601,
    E1602, E1603, E1604, E1605, E1606, E1607, E1608, E1609, E1701, E1702,
    E1703, E1704, E1705, E1706, E1801, E1802, E1803, E1804,
    E1805, E1901, E1902, E1903, E1904, E1905, E1906, E1907,
    E1908, E1909, E1910, E1911, E1912, E1913, E2001, E2201, E2202,
    E2203, E2204, E2205, E2206, E2207, E2208, E2209, E2210, W2001, W2002,
    W2003, W2004, W2005, W2006, W2007, W2008, H3001, H3002,
    H3003, H3004, H3005, R0001, R0002, R0003, R0004, R0005,
}

/// Every registered diagnostic code.
pub const fn all_codes() -> &'static [DiagnosticCode] {
    REGISTERED_CODES
}

/// Diagnostic severity (spec §13.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocks rendering in CLI render mode.
    Error,
    /// Does not block rendering.
    Warning,
    /// Provides guidance.
    Information,
    /// Editor-only suggestion.
    Hint,
}

/// A secondary span attached to a diagnostic for additional context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedSpan {
    pub span: Span,
    pub message: String,
}

/// A machine-readable diagnostic with source span information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable diagnostic code, e.g. `E0012` (spec §26).
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<RelatedSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    /// Construct a diagnostic with an explicit severity.
    pub fn new(
        severity: Severity,
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Diagnostic {
            code: code.as_str(),
            severity,
            message: message.into(),
            span,
            related: Vec::new(),
            help: None,
        }
    }

    /// Construct an error diagnostic.
    pub fn error(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Error, code, message, span)
    }

    /// Construct a warning diagnostic.
    pub fn warning(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Warning, code, message, span)
    }

    /// Attach help text.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Attach a related span.
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(RelatedSpan {
            span,
            message: message.into(),
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{all_codes, DiagnosticCode};

    #[test]
    fn registered_codes_are_unique_and_well_formed() {
        let mut seen = HashSet::new();
        for code in all_codes() {
            let text = code.as_str();
            assert_eq!(text.len(), 5, "{text}");
            assert!(
                matches!(text.as_bytes()[0], b'E' | b'W' | b'H' | b'R'),
                "{text}"
            );
            assert!(
                text[1..].bytes().all(|byte| byte.is_ascii_digit()),
                "{text}"
            );
            assert!(seen.insert(text), "duplicate diagnostic code {text}");
            assert_eq!(DiagnosticCode::parse(text), Some(*code));
        }
    }

    #[test]
    fn spec_diagnostic_catalog_is_registered() {
        let spec = include_str!("../../../docs/ALGRAF_SPEC.md");
        let catalog_start = spec.find("## 26. Diagnostics Catalog").unwrap();
        let catalog_end = spec[catalog_start..].find("## 27.").unwrap() + catalog_start;
        let catalog = &spec[catalog_start..catalog_end];
        let documented = code_literals(catalog);
        let registered: HashSet<&str> = all_codes().iter().map(|code| code.as_str()).collect();

        for code in &documented {
            assert!(
                registered.contains(code.as_str()),
                "{code} is documented but not registered"
            );
        }
        for code in all_codes() {
            assert!(
                documented.contains(code.as_str()),
                "{} is registered but missing from spec §26",
                code.as_str()
            );
        }
    }

    #[test]
    fn production_sources_use_registered_constants() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf();
        let src_root = root.join("crates");
        let mut raw_literals = Vec::new();
        collect_raw_code_literals(&src_root, &mut raw_literals);
        assert!(
            raw_literals.is_empty(),
            "production sources must use algraf_core::codes constants: {raw_literals:#?}"
        );
    }

    fn code_literals(text: &str) -> HashSet<String> {
        let bytes = text.as_bytes();
        let mut out = HashSet::new();
        let mut idx = 0;
        while idx + 5 <= bytes.len() {
            if matches!(bytes[idx], b'E' | b'W' | b'H' | b'R')
                && bytes[idx + 1..idx + 5]
                    .iter()
                    .all(|byte| byte.is_ascii_digit())
            {
                out.insert(text[idx..idx + 5].to_string());
                idx += 5;
            } else {
                idx += 1;
            }
        }
        out
    }

    fn collect_raw_code_literals(path: &Path, out: &mut Vec<String>) {
        let entries = fs::read_dir(path).unwrap_or_else(|err| {
            panic!("failed to read {}: {err}", path.display());
        });
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                collect_raw_code_literals(&path, out);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            if !path
                .components()
                .any(|component| component.as_os_str() == "src")
            {
                continue;
            }
            if path.ends_with("crates/algraf-core/src/diagnostic.rs") {
                continue;
            }
            let source = fs::read_to_string(&path).unwrap();
            for (line, literal) in string_literals(&source) {
                if is_code_literal(&literal) {
                    out.push(format!("{}:{line}: {literal}", path.display()));
                }
            }
        }
    }

    fn string_literals(source: &str) -> Vec<(usize, String)> {
        let bytes = source.as_bytes();
        let mut literals = Vec::new();
        let mut line = 1usize;
        let mut idx = 0usize;
        while idx < bytes.len() {
            match bytes[idx] {
                b'\n' => {
                    line += 1;
                    idx += 1;
                }
                b'/' if bytes.get(idx + 1) == Some(&b'/') => {
                    idx += 2;
                    while idx < bytes.len() && bytes[idx] != b'\n' {
                        idx += 1;
                    }
                }
                b'/' if bytes.get(idx + 1) == Some(&b'*') => {
                    idx += 2;
                    while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                        if bytes[idx] == b'\n' {
                            line += 1;
                        }
                        idx += 1;
                    }
                    idx = (idx + 2).min(bytes.len());
                }
                b'"' => {
                    let start_line = line;
                    idx += 1;
                    let start = idx;
                    while idx < bytes.len() {
                        match bytes[idx] {
                            b'\\' => idx += 2,
                            b'"' => break,
                            b'\n' => {
                                line += 1;
                                idx += 1;
                            }
                            _ => idx += 1,
                        }
                    }
                    if idx <= bytes.len() {
                        literals.push((start_line, source[start..idx].to_string()));
                    }
                    idx += 1;
                }
                _ => idx += 1,
            }
        }
        literals
    }

    fn is_code_literal(literal: &str) -> bool {
        literal.len() == 5
            && matches!(literal.as_bytes()[0], b'E' | b'W' | b'H' | b'R')
            && literal[1..].bytes().all(|byte| byte.is_ascii_digit())
    }
}
