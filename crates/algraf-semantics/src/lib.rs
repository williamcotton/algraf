//! Name resolution, schema-aware validation, IR, geometry registry, and
//! semantic diagnostics.
//!
//! See spec §8 (algebra semantics), §9 (block scope), and §13 (semantic
//! analysis). [`analyze`] is pure: it consumes a parsed tree and a primary data
//! schema, producing [`ir::ChartIr`] and diagnostics.

pub mod analyzer;
pub mod ir;
pub mod planning;
pub mod registry;
mod util;

pub use analyzer::{
    analyze, analyze_chart, analyze_chart_with_tables, analyze_source, analyze_with_tables,
    Analysis,
};
pub use ir::{
    AestheticMapping, AxisSelectorIr, BinClosedIr, ChartIr, ColumnRef, DataSourceIr, DeriveIr,
    FrameIr, GeometryIr, GeometryKind, GuideIr, GuideOverridesIr, PropertyKey, ScaleIr,
    ScaleTargetIr, ScaleTypeIr, SettingValue, SmoothMethodIr, SpaceDataRef, SpaceIr, StatCallIr,
    StatKind, StatOptionsIr, TableDeclIr, ThemeIr, ThemeOverrides, PROPERTY_KEYS,
};
