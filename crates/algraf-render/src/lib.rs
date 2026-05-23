//! Scale training, layout, stats, geometries, and SVG emission.
//!
//! See spec §16 (scales), §17 (layout), §18 (SVG), §19 (guides), §24 (pipeline).
//! [`render`] turns a [`algraf_semantics::ChartIr`] plus a data [`Table`] into a
//! deterministic SVG string.

mod aes;
mod domains;
mod error;
mod geom;
mod guide;
mod layout;
mod projection;
mod render;
mod scale;
mod space;
mod stats;
mod svg;
mod theme;

pub use error::RenderError;
pub use layout::{FacetPanel, Layout, Rect};
pub use render::{render, render_with_tables, RenderResult};
pub use theme::Theme;

// Re-exported for callers that build a table to render against.
pub use algraf_data::Table;
