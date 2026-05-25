//! Render orchestration: IR + data to a deterministic SVG string
//! (spec §24.4, §24.5, §18).
//!
//! The render pipeline is split into explicit stages:
//! derived-table execution, guide/legend discovery, layout and panel planning,
//! spatial projection fitting, geometry emission, guide emission, and final SVG
//! document assembly.

mod common;
mod derived;
mod document;
mod legend;
mod panels;
mod spatial;

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, Table};
use algraf_semantics::ChartIr;

use crate::error::RenderError;
use crate::layout::Layout;
use crate::theme::Theme;

/// The result of rendering: an SVG document plus render diagnostics.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub svg: String,
    pub diagnostics: Vec<Diagnostic>,
    pub layout: Layout,
}

/// Render a chart IR against its primary data table (spec §24.4).
///
/// `theme` is the base (chart-level) theme already resolved by the caller.
/// `cli_theme_override`, if `Some`, replaces space-local theme overrides too
/// (spec §22.3): CLI `--theme` is the strongest source.
pub fn render(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render_with_tables(ir, primary, &HashMap::new(), theme, cli_theme_override)
}

/// Render a chart IR against its primary table plus chart-scoped named tables
/// (spec §10.x). `named_tables` maps each `Table name = "..."` declaration's
/// name to its loaded frame; the caller loads them at the I/O boundary.
pub fn render_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    let mut diagnostics = Vec::new();
    let derived = derived::compute_derived(ir, primary, named_tables);
    let plan = panels::build_render_plan(
        ir,
        primary,
        &derived,
        theme,
        cli_theme_override,
        &mut diagnostics,
    );
    let svg = document::emit_document(
        ir,
        &plan.layout,
        &plan.legends,
        &plan.panels,
        theme,
        &mut diagnostics,
    );

    Ok(RenderResult {
        svg,
        diagnostics,
        layout: plan.layout,
    })
}
