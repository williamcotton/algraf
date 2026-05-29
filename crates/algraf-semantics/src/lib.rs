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
    AestheticMapping, AxisSelectorIr, BinClosedIr, BinIntervalIr, ChartIr, ColumnRef, CoordsIr,
    DataSourceIr, DeriveIr, FrameIr, GeometryIr, GeometryKind, GradientIr, GradientStopIr,
    GridShapeIr, GuideIr, GuideOverridesIr, InteractionIr, PolarThetaIr, PropertyKey, ScaleIr,
    ScaleTargetIr, ScaleTypeIr, SettingValue, SmoothMethodIr, SpaceDataRef, SpaceIr,
    SpatialPredicateIr, StatCallIr, StatKind, StatOptionsIr, TableDeclIr, TemporalFormatIr,
    ThemeIr, ThemeOverrides, PROPERTY_KEYS,
};
pub use planning::{geometry_column_name, spatial_join_appended_columns};
