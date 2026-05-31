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
//! The scene is produced by [`super::panels::build_render_plan`]. As of v0.24.0
//! the seam has two implementations that consume the same scene:
//!
//! - [`SvgBackend`] writes deterministic SVG via [`super::document`] — the
//!   canonical backend of §18.
//! - [`DrawListBackend`](super::draw_list::DrawListBackend) records a serializable,
//!   Canvas-drawable [`DrawList`](super::draw_list::DrawList) of frame primitives.
//!
//! The trait is generic over its `Output` so each backend returns its own
//! serialized form. It remains crate-private and is *not* a plugin API: the set of
//! backends is closed and compiled in (spec §24.6).

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, Table};
use algraf_semantics::ChartIr;
use std::collections::HashMap;

use crate::aes::Legend;
use crate::layout::Layout;
use crate::render::RenderLimits;
use crate::theme::Theme;

use super::document;
use super::metadata::InteractionMetadata;
use super::panels::Panel;

/// A fully planned render scene: everything a backend needs to emit output, with
/// no format-specific decisions remaining. Borrows the plan produced during the
/// planning half so emission allocates only its own output buffer.
pub(super) struct RenderScene<'a> {
    pub(super) ir: &'a ChartIr,
    pub(super) primary: &'a dyn Table,
    pub(super) derived: &'a HashMap<String, DataFrame>,
    pub(super) layout: &'a Layout,
    pub(super) legends: &'a [Legend],
    pub(super) panels: &'a [Panel<'a>],
    pub(super) theme: &'a Theme,
    pub(super) cli_theme_override: Option<&'a str>,
    pub(super) limits: &'a RenderLimits,
}

/// Serializes a planned [`RenderScene`] into one concrete output format.
///
/// This is the render execution boundary: planning code never writes output, and
/// a backend never makes layout or scale decisions. The trait is crate-private
/// with a closed set of implementations; it marks where an additional compiled-in
/// backend attaches, not a public extension point (spec §24.6).
pub(super) trait RenderBackend {
    /// The serialized form this backend produces (e.g. an SVG string or a
    /// [`DrawList`](super::draw_list::DrawList)).
    type Output;

    fn emit(
        &self,
        scene: &RenderScene<'_>,
        metadata: &InteractionMetadata,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self::Output;
}

/// The deterministic SVG backend — the canonical backend (spec §18, §24.6).
///
/// `interactive` opts into the fixed, audited interactive runtime (spec §29.3):
/// when `true`, the emitted document embeds the single Algraf-shipped script that
/// reads the inert per-mark metadata (`<title>` tooltips, `data-algraf-highlight`
/// groups) and rendered plot/axis elements for crosshair value readouts. When
/// `false` (the default), the SVG is byte-for-byte the canonical, script-free
/// output.
pub(super) struct SvgBackend {
    pub(super) interactive: bool,
}

impl RenderBackend for SvgBackend {
    type Output = String;

    fn emit(
        &self,
        scene: &RenderScene<'_>,
        _metadata: &InteractionMetadata,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> String {
        document::emit_document(scene, self.interactive, diagnostics)
    }
}
