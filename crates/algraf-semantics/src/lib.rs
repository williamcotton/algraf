//! Name resolution, schema-aware validation, IR, geometry registry, and
//! semantic diagnostics.
//!
//! See spec §8 (algebra semantics), §9 (block scope), and §13 (semantic
//! analysis). [`analyze`] is pure: it consumes a parsed tree and a primary data
//! schema, producing [`ir::ChartIr`] and diagnostics.

pub mod analyzer;
pub mod ir;
pub mod registry;
mod util;

pub use analyzer::{analyze, analyze_chart, analyze_source, Analysis};
pub use ir::{
    AestheticMapping, AxisSelectorIr, ChartIr, ColumnRef, DataSourceIr, DeriveIr, FrameIr,
    GeometryIr, GeometryKind, GuideIr, GuideOverridesIr, ScaleIr, ScaleTargetIr, ScaleTypeIr,
    SettingValue, SpaceDataRef, SpaceIr, StatKind, ThemeIr, ThemeOverrides,
};
