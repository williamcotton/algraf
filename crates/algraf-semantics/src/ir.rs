//! Semantic IR (spec §13.2–13.7).
//!
//! The IR mirrors executable meaning, separate from the source-mirroring AST.
//! Unknown references use `Invalid` / `Unknown` sentinels to avoid cascading
//! failures (spec §13.7).

use algraf_core::Span;
use algraf_data::DataType;

/// The root of the analyzed chart (spec §13.2).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartIr {
    pub data_source: DataSourceIr,
    /// Chart-scoped named CSV tables declared with `Table name = "..."`
    /// (spec §10.x). The CLI loads each path and supplies the frames to render.
    pub tables: Vec<TableDeclIr>,
    pub derived_tables: Vec<DeriveIr>,
    pub layout: LayoutIr,
    pub guides: GuideIr,
    pub scales: Vec<ScaleIr>,
    pub theme: Option<ThemeIr>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub caption: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Per-side minimum plot margins in pixels (spec §17.3). `None` keeps the
    /// computed default for that side.
    pub margin_top: Option<u32>,
    pub margin_right: Option<u32>,
    pub margin_bottom: Option<u32>,
    pub margin_left: Option<u32>,
    pub spaces: Vec<SpaceIr>,
}

/// A chart-scoped named CSV table declaration (`Table name = "path.csv"`,
/// spec §10.x).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDeclIr {
    pub name: String,
    pub path: String,
    pub span: Span,
}

/// The chart's primary data source (spec §10.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSourceIr {
    /// A CSV path relative to the source file.
    Path(String),
    /// The `stdin` sentinel.
    Stdin,
    /// No valid data source was declared.
    Missing,
}

/// A resolved theme: an optional named base plus override values layered on top
/// (spec §20.1, §20.8).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeIr {
    /// The named base theme (e.g. `"minimal"`), or `None` to inherit.
    pub base: Option<String>,
    /// Per-field overrides applied on top of the base.
    pub overrides: ThemeOverrides,
}

impl ThemeIr {
    /// A theme that only selects a named base, with no overrides.
    pub fn named(name: String) -> ThemeIr {
        ThemeIr {
            base: Some(name),
            overrides: ThemeOverrides::default(),
        }
    }
}

/// Source-level overrides for individual theme fields (spec §20.8). `None`
/// leaves the base theme's value unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeOverrides {
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub background: Option<String>,
    pub plot_background: Option<String>,
    pub axis_color: Option<String>,
    pub grid_major_color: Option<String>,
    pub grid_major_width: Option<f64>,
    pub text_color: Option<String>,
    pub title_size: Option<f64>,
    pub point_size: Option<f64>,
    pub line_width: Option<f64>,
    pub grid: Option<bool>,
    pub axes: Option<bool>,
}

/// Chart-level layout settings that affect viewport allocation (spec §17.4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutIr {
    pub facet_columns: Option<usize>,
}

/// Chart-level guide configuration (spec §19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideIr {
    pub legend: bool,
    /// Whether the fill legend is suppressed (e.g. `Guide(fill: null)`).
    pub fill_legend: bool,
    /// Whether the stroke legend is suppressed (e.g. `Guide(stroke: null)`).
    pub stroke_legend: bool,
    /// Whether grid lines are drawn when the active theme supports grids.
    pub grid: bool,
    /// Override label for the x axis (spec §19.4).
    pub x_label: Option<String>,
    /// Override label for the y axis (spec §19.4).
    pub y_label: Option<String>,
}

impl Default for GuideIr {
    fn default() -> Self {
        GuideIr {
            legend: true,
            fill_legend: true,
            stroke_legend: true,
            grid: true,
            x_label: None,
            y_label: None,
        }
    }
}

impl GuideIr {
    pub fn with_overrides(&self, overrides: &GuideOverridesIr) -> GuideIr {
        GuideIr {
            legend: overrides.legend.unwrap_or(self.legend),
            fill_legend: overrides.fill_legend.unwrap_or(self.fill_legend),
            stroke_legend: overrides.stroke_legend.unwrap_or(self.stroke_legend),
            grid: overrides.grid.unwrap_or(self.grid),
            x_label: overrides.x_label.clone().or_else(|| self.x_label.clone()),
            y_label: overrides.y_label.clone().or_else(|| self.y_label.clone()),
        }
    }
}

/// Space-local guide overrides. `None` means inherit chart-level behavior.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuideOverridesIr {
    pub legend: Option<bool>,
    pub fill_legend: Option<bool>,
    pub stroke_legend: Option<bool>,
    pub grid: Option<bool>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
}

/// A source-level scale declaration (spec §16.11).
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleIr {
    pub target: ScaleTargetIr,
    pub scale_type: Option<ScaleTypeIr>,
    /// Numeric domain bounds. Each element may be `None`, meaning "infer this
    /// bound from the data" (e.g. `domain: [0, null]`, spec §16.11).
    pub domain: Option<[Option<f64>; 2]>,
    /// Numeric output range for a `size`/`strokeWidth` scale (spec §16.8,
    /// §16.11). Each element may be `None` to infer from the data.
    pub range: Option<[Option<f64>; 2]>,
    pub reverse: Option<bool>,
    /// Constrain axis ticks to whole integers (spec §16.10). Applies only to
    /// continuous axis scales.
    pub integer: Option<bool>,
    pub palette: Option<String>,
    pub gradient: Option<Vec<String>>,
    /// A manual category → color map for a categorical `fill`/`stroke` scale
    /// (`range: ["A" => "burlywood"]`, spec §16.13). Order defines category and
    /// legend-entry order.
    pub color_map: Option<Vec<(String, String)>>,
    /// A manual category → display-label map (`labels: ["A" => "Advance"]`,
    /// spec §16.13). Aligned with `color_map` order when both are present.
    pub label_map: Option<Vec<(String, String)>>,
    /// An explicit legend title that overrides the column-derived default for a
    /// `fill`/`stroke` aesthetic scale (spec §16.13).
    pub label: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaleTargetIr {
    Axis(AxisSelectorIr),
    Aesthetic {
        aesthetic: String,
        column: Option<ColumnRef>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSelectorIr {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleTypeIr {
    Linear,
    Log10,
}

/// A derived table produced by a `Derive` declaration (spec §13.4).
#[derive(Debug, Clone, PartialEq)]
pub struct DeriveIr {
    pub name: String,
    pub data: SpaceDataRef,
    pub stat: StatCallIr,
    pub output_schema: Vec<ColumnDefIr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatCallIr {
    pub kind: StatKind,
    pub input: FrameIr,
    pub settings: Vec<Setting>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKind {
    Bin,
    Bin2D,
    HexBin,
    Count,
    Smooth,
    Boxplot,
    Density,
}

/// A minimal column definition carried in the IR (name + type + span).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDefIr {
    pub name: String,
    pub dtype: DataType,
}

/// A space and its trained-frame plan (spec §13.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceIr {
    pub data: SpaceDataRef,
    pub frame: FrameIr,
    pub geometries: Vec<GeometryIr>,
    pub guides: GuideOverridesIr,
    pub scales: Vec<ScaleIr>,
    /// Space-local theme override (spec §7.3, §22.3). When set, this theme
    /// overrides the chart-level theme for this space only.
    pub theme: Option<ThemeIr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceDataRef {
    Primary,
    Derived(String),
    /// A chart-scoped named CSV table (`Table cities = "..."`, spec §10.x).
    Table(String),
}

/// The algebraic frame in canonical form (spec §13.5, §8.9).
#[derive(Debug, Clone, PartialEq)]
pub enum FrameIr {
    Vector(ColumnRef),
    Cartesian(Vec<FrameIr>),
    Nested {
        outer: Box<FrameIr>,
        inner: Box<FrameIr>,
    },
    Union(Vec<FrameIr>),
    Invalid,
}

/// A resolved column reference (spec §13.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub name: String,
    pub dtype: DataType,
    pub span: Span,
}

/// A geometry layer (spec §13.6).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryIr {
    pub kind: GeometryKind,
    pub mappings: Vec<AestheticMapping>,
    pub settings: Vec<GeometrySetting>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryKind {
    Point,
    Line,
    Path,
    Bar,
    Rect,
    Histogram,
    FreqPoly,
    Bin2D,
    HexBin,
    Smooth,
    Boxplot,
    Violin,
    Density,
    Ribbon,
    Tile,
    HLine,
    VLine,
    Rug,
    Area,
    Text,
    Segment,
}

/// A binding from an aesthetic to a data column (spec §13.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AestheticMapping {
    pub aesthetic: String,
    pub column: ColumnRef,
}

/// A geometry setting bound to a literal value.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometrySetting {
    pub name: String,
    pub value: SettingValue,
}

/// A general statistical-transform or geometry setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub name: String,
    pub value: SettingValue,
}

/// A literal setting value.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
    NumberArray(Vec<f64>),
}
