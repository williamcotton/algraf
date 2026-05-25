//! The output-backend seam (spec §24.6).
//!
//! Rendering is split into two halves with an explicit boundary between them:
//!
//! 1. **Planning** builds a [`RenderScene`]: a fully resolved description of what
//!    to draw — layout rectangles, trained scales, legends, and per-panel
//!    geometry — with no output-format decisions baked in.
//! 2. **Emission** hands that scene to a [`RenderBackend`], which serializes it to
//!    bytes for one concrete output format.
//!
//! In v0.17.0 the scene is produced by [`super::panels::build_render_plan`] and
//! the only backend is [`SvgBackend`], which writes deterministic SVG via
//! [`super::document`]. The trait exists to name the seam — so a future raster or
//! canvas backend has an obvious insertion point — not to expose a plugin API; it
//! is private to the crate and has exactly one implementation.

use algraf_core::Diagnostic;
use algraf_semantics::ChartIr;

use crate::aes::Legend;
use crate::layout::Layout;
use crate::theme::Theme;

use super::document;
use super::panels::Panel;

/// A fully planned render scene: everything a backend needs to emit output, with
/// no format-specific decisions remaining. Borrows the plan produced during the
/// planning half so emission allocates only its own output buffer.
pub(super) struct RenderScene<'a> {
    pub(super) ir: &'a ChartIr,
    pub(super) layout: &'a Layout,
    pub(super) legends: &'a [Legend],
    pub(super) panels: &'a [Panel<'a>],
    pub(super) theme: &'a Theme,
}

/// Serializes a planned [`RenderScene`] to bytes for one output format.
///
/// This is the render execution boundary: planning code never writes output, and
/// a backend never makes layout or scale decisions. The trait is private and has
/// a single implementation ([`SvgBackend`]); it marks where an additional backend
/// would attach, not a public extension point.
pub(super) trait RenderBackend {
    fn emit(&self, scene: &RenderScene<'_>, diagnostics: &mut Vec<Diagnostic>) -> String;
}

/// The deterministic SVG backend — the only backend in v0.17.0 (spec §18, §24.6).
pub(super) struct SvgBackend;

impl RenderBackend for SvgBackend {
    fn emit(&self, scene: &RenderScene<'_>, diagnostics: &mut Vec<Diagnostic>) -> String {
        document::emit_document(scene, diagnostics)
    }
}
