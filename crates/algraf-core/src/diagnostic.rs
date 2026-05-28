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

/// Canonical diagnostic-code constants (spec §26).
pub mod codes {
    use super::DiagnosticCode;

    pub const E0001: DiagnosticCode = DiagnosticCode::new("E0001");
    pub const E0002: DiagnosticCode = DiagnosticCode::new("E0002");
    pub const E0003: DiagnosticCode = DiagnosticCode::new("E0003");
    pub const E0004: DiagnosticCode = DiagnosticCode::new("E0004");
    pub const E0005: DiagnosticCode = DiagnosticCode::new("E0005");
    pub const E0006: DiagnosticCode = DiagnosticCode::new("E0006");
    pub const E0007: DiagnosticCode = DiagnosticCode::new("E0007");
    pub const E0008: DiagnosticCode = DiagnosticCode::new("E0008");
    pub const E0009: DiagnosticCode = DiagnosticCode::new("E0009");
    pub const E0010: DiagnosticCode = DiagnosticCode::new("E0010");
    pub const E0011: DiagnosticCode = DiagnosticCode::new("E0011");
    pub const E0012: DiagnosticCode = DiagnosticCode::new("E0012");
    pub const E0013: DiagnosticCode = DiagnosticCode::new("E0013");
    pub const E0014: DiagnosticCode = DiagnosticCode::new("E0014");
    pub const E0015: DiagnosticCode = DiagnosticCode::new("E0015");
    pub const E0016: DiagnosticCode = DiagnosticCode::new("E0016");
    pub const E0017: DiagnosticCode = DiagnosticCode::new("E0017");
    pub const E0018: DiagnosticCode = DiagnosticCode::new("E0018");
    pub const E0019: DiagnosticCode = DiagnosticCode::new("E0019");
    pub const E0020: DiagnosticCode = DiagnosticCode::new("E0020");
    pub const E0021: DiagnosticCode = DiagnosticCode::new("E0021");
    pub const E0022: DiagnosticCode = DiagnosticCode::new("E0022");
    pub const E0023: DiagnosticCode = DiagnosticCode::new("E0023");
    pub const E0024: DiagnosticCode = DiagnosticCode::new("E0024");
    pub const E0025: DiagnosticCode = DiagnosticCode::new("E0025");

    pub const E1001: DiagnosticCode = DiagnosticCode::new("E1001");
    pub const E1002: DiagnosticCode = DiagnosticCode::new("E1002");
    pub const E1003: DiagnosticCode = DiagnosticCode::new("E1003");
    pub const E1004: DiagnosticCode = DiagnosticCode::new("E1004");
    pub const E1005: DiagnosticCode = DiagnosticCode::new("E1005");
    pub const E1006: DiagnosticCode = DiagnosticCode::new("E1006");
    pub const E1007: DiagnosticCode = DiagnosticCode::new("E1007");
    pub const E1008: DiagnosticCode = DiagnosticCode::new("E1008");
    pub const E1009: DiagnosticCode = DiagnosticCode::new("E1009");
    pub const E1010: DiagnosticCode = DiagnosticCode::new("E1010");
    pub const E1011: DiagnosticCode = DiagnosticCode::new("E1011");
    pub const E1012: DiagnosticCode = DiagnosticCode::new("E1012");
    pub const E1013: DiagnosticCode = DiagnosticCode::new("E1013");
    pub const E1014: DiagnosticCode = DiagnosticCode::new("E1014");
    pub const E1015: DiagnosticCode = DiagnosticCode::new("E1015");
    pub const E1016: DiagnosticCode = DiagnosticCode::new("E1016");
    pub const E1101: DiagnosticCode = DiagnosticCode::new("E1101");
    pub const E1102: DiagnosticCode = DiagnosticCode::new("E1102");
    pub const E1103: DiagnosticCode = DiagnosticCode::new("E1103");
    pub const E1104: DiagnosticCode = DiagnosticCode::new("E1104");
    pub const E1105: DiagnosticCode = DiagnosticCode::new("E1105");
    pub const E1106: DiagnosticCode = DiagnosticCode::new("E1106");
    pub const E1107: DiagnosticCode = DiagnosticCode::new("E1107");
    pub const E1108: DiagnosticCode = DiagnosticCode::new("E1108");
    pub const E1201: DiagnosticCode = DiagnosticCode::new("E1201");
    pub const E1202: DiagnosticCode = DiagnosticCode::new("E1202");
    pub const E1203: DiagnosticCode = DiagnosticCode::new("E1203");
    pub const E1204: DiagnosticCode = DiagnosticCode::new("E1204");
    pub const E1205: DiagnosticCode = DiagnosticCode::new("E1205");
    pub const E1301: DiagnosticCode = DiagnosticCode::new("E1301");
    pub const E1302: DiagnosticCode = DiagnosticCode::new("E1302");
    pub const E1303: DiagnosticCode = DiagnosticCode::new("E1303");
    pub const E1304: DiagnosticCode = DiagnosticCode::new("E1304");
    pub const E1305: DiagnosticCode = DiagnosticCode::new("E1305");
    pub const E1306: DiagnosticCode = DiagnosticCode::new("E1306");
    pub const E1401: DiagnosticCode = DiagnosticCode::new("E1401");
    pub const E1402: DiagnosticCode = DiagnosticCode::new("E1402");
    pub const E1403: DiagnosticCode = DiagnosticCode::new("E1403");
    pub const E1404: DiagnosticCode = DiagnosticCode::new("E1404");
    pub const E1405: DiagnosticCode = DiagnosticCode::new("E1405");
    pub const E1501: DiagnosticCode = DiagnosticCode::new("E1501");
    pub const E1601: DiagnosticCode = DiagnosticCode::new("E1601");
    pub const E1602: DiagnosticCode = DiagnosticCode::new("E1602");
    pub const E1603: DiagnosticCode = DiagnosticCode::new("E1603");
    pub const E1604: DiagnosticCode = DiagnosticCode::new("E1604");
    pub const E1605: DiagnosticCode = DiagnosticCode::new("E1605");
    pub const E1606: DiagnosticCode = DiagnosticCode::new("E1606");
    pub const E1607: DiagnosticCode = DiagnosticCode::new("E1607");
    pub const E1701: DiagnosticCode = DiagnosticCode::new("E1701");
    pub const E1702: DiagnosticCode = DiagnosticCode::new("E1702");
    pub const E1703: DiagnosticCode = DiagnosticCode::new("E1703");
    pub const E1704: DiagnosticCode = DiagnosticCode::new("E1704");
    pub const E1705: DiagnosticCode = DiagnosticCode::new("E1705");
    pub const E1706: DiagnosticCode = DiagnosticCode::new("E1706");
    pub const E1801: DiagnosticCode = DiagnosticCode::new("E1801");
    pub const E1802: DiagnosticCode = DiagnosticCode::new("E1802");
    pub const E1803: DiagnosticCode = DiagnosticCode::new("E1803");
    pub const E1804: DiagnosticCode = DiagnosticCode::new("E1804");
    pub const E1805: DiagnosticCode = DiagnosticCode::new("E1805");
    pub const E1901: DiagnosticCode = DiagnosticCode::new("E1901");
    pub const E1902: DiagnosticCode = DiagnosticCode::new("E1902");
    pub const E1903: DiagnosticCode = DiagnosticCode::new("E1903");
    pub const E1904: DiagnosticCode = DiagnosticCode::new("E1904");
    pub const E1905: DiagnosticCode = DiagnosticCode::new("E1905");
    pub const E1906: DiagnosticCode = DiagnosticCode::new("E1906");
    pub const E1907: DiagnosticCode = DiagnosticCode::new("E1907");

    pub const W2001: DiagnosticCode = DiagnosticCode::new("W2001");
    pub const W2002: DiagnosticCode = DiagnosticCode::new("W2002");
    pub const W2003: DiagnosticCode = DiagnosticCode::new("W2003");
    pub const W2004: DiagnosticCode = DiagnosticCode::new("W2004");
    pub const W2005: DiagnosticCode = DiagnosticCode::new("W2005");
    pub const W2006: DiagnosticCode = DiagnosticCode::new("W2006");
    pub const W2007: DiagnosticCode = DiagnosticCode::new("W2007");
    pub const W2008: DiagnosticCode = DiagnosticCode::new("W2008");

    pub const H3001: DiagnosticCode = DiagnosticCode::new("H3001");
    pub const H3002: DiagnosticCode = DiagnosticCode::new("H3002");
    pub const H3003: DiagnosticCode = DiagnosticCode::new("H3003");
    pub const H3004: DiagnosticCode = DiagnosticCode::new("H3004");
    pub const H3005: DiagnosticCode = DiagnosticCode::new("H3005");

    pub const R0001: DiagnosticCode = DiagnosticCode::new("R0001");
    pub const R0002: DiagnosticCode = DiagnosticCode::new("R0002");
    pub const R0003: DiagnosticCode = DiagnosticCode::new("R0003");
    pub const R0004: DiagnosticCode = DiagnosticCode::new("R0004");
}

/// Every registered diagnostic code.
pub const fn all_codes() -> &'static [DiagnosticCode] {
    &[
        codes::E0001,
        codes::E0002,
        codes::E0003,
        codes::E0004,
        codes::E0005,
        codes::E0006,
        codes::E0007,
        codes::E0008,
        codes::E0009,
        codes::E0010,
        codes::E0011,
        codes::E0012,
        codes::E0013,
        codes::E0014,
        codes::E0015,
        codes::E0016,
        codes::E0017,
        codes::E0018,
        codes::E0019,
        codes::E0020,
        codes::E0021,
        codes::E0022,
        codes::E0023,
        codes::E0024,
        codes::E0025,
        codes::E1001,
        codes::E1002,
        codes::E1003,
        codes::E1004,
        codes::E1005,
        codes::E1006,
        codes::E1007,
        codes::E1008,
        codes::E1009,
        codes::E1010,
        codes::E1011,
        codes::E1012,
        codes::E1013,
        codes::E1014,
        codes::E1015,
        codes::E1016,
        codes::E1101,
        codes::E1102,
        codes::E1103,
        codes::E1104,
        codes::E1105,
        codes::E1106,
        codes::E1107,
        codes::E1108,
        codes::E1201,
        codes::E1202,
        codes::E1203,
        codes::E1204,
        codes::E1205,
        codes::E1301,
        codes::E1302,
        codes::E1303,
        codes::E1304,
        codes::E1305,
        codes::E1306,
        codes::E1401,
        codes::E1402,
        codes::E1403,
        codes::E1404,
        codes::E1405,
        codes::E1501,
        codes::E1601,
        codes::E1602,
        codes::E1603,
        codes::E1604,
        codes::E1605,
        codes::E1606,
        codes::E1607,
        codes::E1701,
        codes::E1702,
        codes::E1703,
        codes::E1704,
        codes::E1705,
        codes::E1706,
        codes::E1801,
        codes::E1802,
        codes::E1803,
        codes::E1804,
        codes::E1805,
        codes::E1901,
        codes::E1902,
        codes::E1903,
        codes::E1904,
        codes::E1905,
        codes::E1906,
        codes::E1907,
        codes::W2001,
        codes::W2002,
        codes::W2003,
        codes::W2004,
        codes::W2005,
        codes::W2006,
        codes::W2007,
        codes::W2008,
        codes::H3001,
        codes::H3002,
        codes::H3003,
        codes::H3004,
        codes::H3005,
        codes::R0001,
        codes::R0002,
        codes::R0003,
        codes::R0004,
    ]
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
