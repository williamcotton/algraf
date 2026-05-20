//! SVG output helpers: deterministic number formatting and XML escaping
//! (spec §18.8, §18.9).

use std::fmt::Write;

/// Format a float for SVG output: up to 3 decimal places, locale-independent,
/// with trailing zeros and a trailing decimal point trimmed (spec §18.8).
pub fn num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 1000.0).round() / 1000.0;
    // Normalize negative zero to zero.
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let mut s = format!("{rounded:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

/// Format an internal identifier as a human-facing guide label.
pub fn display_label(label: &str) -> String {
    label.replace('_', " ")
}

/// Escape text content for an XML text node (spec §18.9).
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Escape a value for an XML attribute (spec §18.9).
pub fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// A small incremental SVG writer that tracks indentation.
#[derive(Default)]
pub struct SvgWriter {
    buf: String,
    depth: usize,
}

impl SvgWriter {
    pub fn new() -> Self {
        SvgWriter {
            buf: String::new(),
            depth: 0,
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.buf.push_str("  ");
        }
    }

    /// Write a full line at the current indent.
    pub fn line(&mut self, text: &str) {
        self.indent();
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    /// Open a group `<g ...>` and increase indentation.
    pub fn open_group(&mut self, attrs: &str) {
        self.indent();
        if attrs.is_empty() {
            self.buf.push_str("<g>\n");
        } else {
            let _ = writeln!(self.buf, "<g {attrs}>");
        }
        self.depth += 1;
    }

    /// Close a group `</g>`.
    pub fn close_group(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.indent();
        self.buf.push_str("</g>\n");
    }

    pub fn finish(self) -> String {
        self.buf
    }
}
