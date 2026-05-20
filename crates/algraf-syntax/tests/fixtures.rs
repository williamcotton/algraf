//! Grammar fixture corpus tests (spec §27.3).
//!
//! Every `.ag` file under `tests/fixtures/parser/valid` must parse without parse
//! diagnostics; every file under `invalid` must produce at least one diagnostic.
//! Both sets must round-trip losslessly and never panic.

use std::fs;
use std::path::Path;

use algraf_syntax::parse;

fn fixtures_in(subdir: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parser")
        .join(subdir);
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("fixture dir exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("ag") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, fs::read_to_string(&path).unwrap()));
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no fixtures found in {subdir}");
    out
}

#[test]
fn test_valid_fixtures_parse_clean() {
    for (name, source) in fixtures_in("valid") {
        let parsed = parse(&source);
        assert!(
            parsed.diagnostics().is_empty(),
            "{name} should parse without diagnostics, got: {:?}",
            parsed.diagnostics()
        );
        // The CST round-trips to the exact source.
        assert_eq!(
            parsed.syntax().to_string(),
            source,
            "{name} is not lossless"
        );
    }
}

#[test]
fn test_invalid_fixtures_produce_diagnostics() {
    for (name, source) in fixtures_in("invalid") {
        let parsed = parse(&source);
        assert!(
            !parsed.diagnostics().is_empty(),
            "{name} should produce diagnostics"
        );
        // Even invalid input round-trips losslessly and does not panic.
        assert_eq!(
            parsed.syntax().to_string(),
            source,
            "{name} is not lossless"
        );
    }
}
