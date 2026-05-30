//! Self-contained WASI command: render an embedded chart to SVG on stdout.
//!
//! This is the vertical-slice proof that the Algraf pipeline runs on
//! `wasm32`. It embeds an `.ag` source plus its CSV at build time, renders
//! entirely in-memory (no filesystem), and writes the SVG to stdout so a host
//! (`node:wasi`, `wasmtime`, …) can capture it. The browser binding wraps the
//! same `algraf_wasm::render_to_svg` entry point.

use std::collections::HashMap;

const SOURCE: &str = r#"Chart(data: "penguins.csv", width: 760, height: 500) {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.82, size: 4)
    }
}
"#;

const PENGUINS_CSV: &str = include_str!("../../../../examples/penguins.csv");

fn main() {
    let mut files = HashMap::new();
    files.insert("penguins.csv".to_string(), PENGUINS_CSV.as_bytes().to_vec());

    let outcome = algraf_wasm::render_to_svg(SOURCE, files);

    for diag in &outcome.diagnostics {
        eprintln!("{}: {}", diag.code, diag.message);
    }
    if let Some(error) = &outcome.error {
        eprintln!("render error: {error}");
    }

    match outcome.svg {
        Some(svg) => print!("{svg}"),
        None => {
            eprintln!("no SVG produced");
            std::process::exit(1);
        }
    }
}
