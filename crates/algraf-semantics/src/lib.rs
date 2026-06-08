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
    analyze_with_tables_and_options, Analysis, AnalysisOptions,
};
pub use ir::{
    AestheticMapping, AxisSelectorIr, AxisViewDomainIr, BinClosedIr, BinIntervalIr, ChartIr,
    ColumnRef, CoordinateViewIr, CoordsIr, DataSourceIr, DeriveIr, FacetGridIr, FacetLabelModeIr,
    FacetScaleModeIr, FrameIr, GeometryIr, GeometryKind, GlyphCallIr, GlyphClipIr, GlyphHostRefIr,
    GlyphKeyIr, GlyphPlacementIr, GlyphScalePolicyIr, GlyphSizeIr, GradientIr, GradientStopIr,
    GridBinsIr, GridShapeIr, GuideIr, GuideOverridesIr, InteractionIr, IntervalOrientationIr,
    LegendPositionIr, LevelSpecIr, PanelSpacingIr, PolarDirectionIr, PolarThetaIr, PropertyKey,
    QqDistributionIr, ScaleExpansionIr, ScaleIr, ScaleModeIr, ScaleTargetIr, ScaleTypeIr,
    SettingValue, SmoothMethodIr, SpaceDataRef, SpaceIr, SpaceLayerIr, SpatialPredicateIr,
    StatCallIr, StatKind, StatOptionsIr, StepDirectionIr, SummaryReducerIr, TableDeclIr,
    TemporalFormatIr, ThemeIr, ThemeLineIr, ThemeOverrides, ThemeRectIr, ThemeTextIr,
    PROPERTY_KEYS,
};
pub use planning::{geometry_column_name, spatial_join_appended_columns};
