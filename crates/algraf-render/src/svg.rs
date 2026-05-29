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

/// One XML attribute for structured SVG emission.
#[derive(Debug, Clone)]
pub struct SvgAttr {
    name: &'static str,
    value: String,
}

impl SvgAttr {
    pub fn new(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }

    pub fn number(name: &'static str, value: f64) -> Self {
        Self::new(name, num(value))
    }
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

    /// Write an empty element with escaped attributes, preserving attribute
    /// order.
    pub fn empty_element(&mut self, name: &str, attrs: &[SvgAttr]) {
        self.indent();
        write!(self.buf, "<{name}").expect("writing to String cannot fail");
        self.write_attrs(attrs);
        self.buf.push_str(" />\n");
    }

    /// Write a text element with escaped attributes and escaped text content.
    pub fn text_element(&mut self, name: &str, attrs: &[SvgAttr], text: &str) {
        self.indent();
        write!(self.buf, "<{name}").expect("writing to String cannot fail");
        self.write_attrs(attrs);
        self.buf.push('>');
        self.buf.push_str(&escape_text(text));
        writeln!(self.buf, "</{name}>").expect("writing to String cannot fail");
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

    fn write_attrs(&mut self, attrs: &[SvgAttr]) {
        for attr in attrs {
            write!(self.buf, " {}=\"{}\"", attr.name, escape_attr(&attr.value))
                .expect("writing to String cannot fail");
        }
    }
}
