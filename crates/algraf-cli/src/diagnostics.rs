//! Diagnostic rendering: human-readable and JSON (spec §22.10).

use algraf_core::{Diagnostic, Severity};
use serde_json::{json, Value};

/// A zero-based line and UTF-16 character position (LSP convention).
struct Position {
    line: usize,
    character: usize,
}

/// Convert a byte offset to a zero-based line / UTF-16 character position.
fn position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0;
    let mut line_start = 0;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    let character = source[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum();
    Position { line, character }
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "information",
        Severity::Hint => "hint",
    }
}

/// Whether any diagnostic should fail the command. With `strict`, warnings also
/// count (spec §13.16, §22.3).
pub fn has_blocking(diagnostics: &[Diagnostic], strict: bool) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error || (strict && d.severity == Severity::Warning))
}

/// Render diagnostics in a human-readable form for the terminal (spec §22.10).
pub fn render_human(source: &str, file: &str, diagnostics: &[Diagnostic]) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out = String::new();
    for d in diagnostics {
        let pos = position(source, d.span.start);
        let line_no = pos.line + 1;
        let col = source[line_start(source, d.span.start)..d.span.start]
            .chars()
            .count()
            + 1;
        out.push_str(&format!(
            "{file}:{line_no}:{col}: {}[{}] {}\n",
            severity_str(d.severity),
            d.code,
            d.message,
        ));
        if let Some(text) = lines.get(pos.line) {
            out.push_str(&format!("  {line_no} | {text}\n"));
            let pad = " ".repeat(col - 1);
            let width = (d.span.end - d.span.start).max(1);
            let caret = "^".repeat(width);
            let gutter = " ".repeat(line_no.to_string().len());
            out.push_str(&format!("  {gutter} | {pad}{caret}\n"));
        }
        if let Some(help) = &d.help {
            out.push_str(&format!("  = help: {help}\n"));
        }
    }
    out
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// Render diagnostics as a JSON array with the stable shape (spec §22.10).
pub fn render_json(source: &str, file: &str, diagnostics: &[Diagnostic]) -> String {
    let items: Vec<Value> = diagnostics
        .iter()
        .map(|d| {
            let start = position(source, d.span.start);
            let end = position(source, d.span.end);
            let related: Vec<Value> = d
                .related
                .iter()
                .map(|r| {
                    json!({
                        "span": { "start": r.span.start, "end": r.span.end },
                        "message": r.message,
                    })
                })
                .collect();
            json!({
                "source": "algraf",
                "code": d.code,
                "severity": severity_str(d.severity),
                "message": d.message,
                "file": file,
                "span": { "start": d.span.start, "end": d.span.end },
                "range": {
                    "start": { "line": start.line, "character": start.character },
                    "end": { "line": end.line, "character": end.character },
                },
                "related": related,
                "help": d.help,
            })
        })
        .collect();
    serde_json::to_string_pretty(&Value::Array(items)).unwrap_or_else(|_| "[]".to_string())
}
