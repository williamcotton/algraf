//! Scale training, layout, stats, geometries, and SVG emission.
//!
//! See spec §16 (scales), §17 (layout), §18 (SVG), §19 (guides), §24 (pipeline).
//! [`render`] turns a [`algraf_semantics::ChartIr`] plus a data [`Table`] into a
//! deterministic SVG string.
//!
//! # Render execution boundary (spec §24.6)
//!
//! The crate is organized around one boundary, between *planning* and *emission*:
//!
//! - **Planning** consumes the IR and loaded data eagerly and resolves a fully
//!   described scene — derived tables ([`render`]'s `derived` step), trained
//!   scales ([`scale`], [`domains`], [`space`]), layout rectangles ([`layout`]),
//!   guide measurements ([`guide`]'s planning half), and legends. It reads data
//!   only through the [`Table`] abstraction and writes no output bytes.
//! - **Emission** takes that scene and serializes it through one of a closed set
//!   of output backends. The canonical SVG backend writes SVG: geometry
//!   ([`geom`]) and guide emission write bytes via the [`svg`] writer and make no
//!   layout or scale decisions. A second backend records a serializable
//!   [`DrawList`] of Canvas-drawable frame primitives ([`render_draw_list`]).
//!
//! Data materialization is eager: stats and scale training run during planning
//! against in-memory tables. Lazy/streaming execution is deferred (see
//! `docs/V0_17_PLAN.md`); the draw-list backend landed in v0.24 (see
//! `docs/V0_24_PLAN.md`).

mod aes;
mod domains;
mod embed;
mod error;
mod geo_stats;
mod geom;
mod guide;
mod helpers;
mod layout;
mod projection;
mod render;
mod scale;
mod space;
mod stats;
mod svg;
mod theme;

pub use embed::{
    render_embedded, render_embedded_json, render_embedded_with_io, EmbeddedOutputFormat,
    EmbeddedRenderError, EmbeddedRenderOptions, EmbeddedRenderResult, InMemoryDriverIo,
    InputOnlyIo,
};
pub use error::RenderError;
pub use layout::{FacetPanel, Layout, Rect};
pub use render::{
    render, render_draw_list, render_draw_list_with_tables, render_with_tables, DrawList,
    DrawListResult, DrawOp, DrawRole, RenderResult, TextAnchor,
};
pub use svg::num as svg_num;
pub use theme::Theme;

// Re-exported for callers that build a table to render against.
pub use algraf_data::Table;
