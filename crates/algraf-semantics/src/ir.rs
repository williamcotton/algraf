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
    pub query: Option<String>,
    pub span: Span,
}

/// The chart's primary data source (spec §10.1, §10.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSourceIr {
    /// A tabular path relative to the source file (CSV/TSV/JSON/NDJSON, chosen
    /// by extension).
    Path(String),
    /// A `GeoJson("path")` source constructor (spec §10.11).
    GeoJson(String),
    /// A `Shapefile("path.shp")` source constructor (spec §10.11).
    Shapefile(String),
    /// A `Sqlite("path.db", "SELECT ... ORDER BY ...")` source constructor
    /// (spec §10.12).
    Sqlite { path: String, query: String },
    /// A `TopoJson("path.topojson", object: "name")` source constructor; `object`
    /// is `None` when the topology's sole object is used (spec §10.11).
    TopoJson {
        path: String,
        object: Option<String>,
    },
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
#[derive(Debug, Clone, PartialEq)]
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
    /// Optional temporal label format for the x axis.
    pub x_time_format: Option<TemporalFormatIr>,
    /// Optional temporal label format for the y axis.
    pub y_time_format: Option<TemporalFormatIr>,
    /// Optional x tick label rotation in degrees (spec §19.4).
    pub x_tick_label_angle: Option<f64>,
    /// Optional y tick label rotation in degrees (spec §19.4).
    pub y_tick_label_angle: Option<f64>,
    /// Polar grid shape (spec §16.16, §19): concentric circles (default) or
    /// straight-edged polygons (radar). Ignored for Cartesian spaces.
    pub grid_shape: GridShapeIr,
}

/// The shape of a polar grid (spec §16.16, §19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridShapeIr {
    /// Concentric circle rings and arc gridlines.
    #[default]
    Circle,
    /// Straight segments between adjacent spokes (radar pentagon/hexagon).
    Polygon,
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
            x_time_format: None,
            y_time_format: None,
            x_tick_label_angle: None,
            y_tick_label_angle: None,
            grid_shape: GridShapeIr::Circle,
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
            x_time_format: overrides
                .x_time_format
                .clone()
                .or_else(|| self.x_time_format.clone()),
            y_time_format: overrides
                .y_time_format
                .clone()
                .or_else(|| self.y_time_format.clone()),
            x_tick_label_angle: overrides.x_tick_label_angle.or(self.x_tick_label_angle),
            y_tick_label_angle: overrides.y_tick_label_angle.or(self.y_tick_label_angle),
            grid_shape: overrides.grid_shape.unwrap_or(self.grid_shape),
        }
    }
}

/// Space-local guide overrides. `None` means inherit chart-level behavior.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuideOverridesIr {
    pub legend: Option<bool>,
    pub fill_legend: Option<bool>,
    pub stroke_legend: Option<bool>,
    pub grid: Option<bool>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub x_time_format: Option<TemporalFormatIr>,
    pub y_time_format: Option<TemporalFormatIr>,
    pub x_tick_label_angle: Option<f64>,
    pub y_tick_label_angle: Option<f64>,
    pub grid_shape: Option<GridShapeIr>,
}

/// Named temporal label formats accepted by `Guide(timeFormat: ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalFormatIr {
    IsoDate,
    IsoMinute,
    IsoSecond,
    IsoMillis,
    Rfc3339,
    Year,
    Month,
    MonthDay,
    TimeMinute,
    TimeSecond,
    Custom(String),
}

impl TemporalFormatIr {
    pub fn as_str(&self) -> &str {
        match self {
            TemporalFormatIr::IsoDate => "iso-date",
            TemporalFormatIr::IsoMinute => "iso-minute",
            TemporalFormatIr::IsoSecond => "iso-second",
            TemporalFormatIr::IsoMillis => "iso-millis",
            TemporalFormatIr::Rfc3339 => "rfc3339",
            TemporalFormatIr::Year => "year",
            TemporalFormatIr::Month => "month",
            TemporalFormatIr::MonthDay => "month-day",
            TemporalFormatIr::TimeMinute => "time-minute",
            TemporalFormatIr::TimeSecond => "time-second",
            TemporalFormatIr::Custom(pattern) => pattern,
        }
    }
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
    pub gradient: Option<GradientIr>,
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

/// Continuous color-gradient stops.
#[derive(Debug, Clone, PartialEq)]
pub enum GradientIr {
    /// Existing evenly spaced color-string stops.
    Even(Vec<String>),
    /// Explicit domain-value stops.
    Positioned(Vec<GradientStopIr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradientStopIr {
    pub value: f64,
    pub color: String,
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

impl AxisSelectorIr {
    /// The authoritative source spelling (`"x"` / `"y"`).
    pub fn as_str(self) -> &'static str {
        match self {
            AxisSelectorIr::X => "x",
            AxisSelectorIr::Y => "y",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleTypeIr {
    Linear,
    Log10,
    Sqrt,
}

impl ScaleTypeIr {
    /// The authoritative source spelling (`"linear"` / `"log10"` / `"sqrt"`),
    /// matching [`crate::registry::SCALE_TYPE_NAMES`].
    pub fn as_str(self) -> &'static str {
        match self {
            ScaleTypeIr::Linear => "linear",
            ScaleTypeIr::Log10 => "log10",
            ScaleTypeIr::Sqrt => "sqrt",
        }
    }
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
    /// Typed, validated stat options (spec §13.4). Replaces the former
    /// string-keyed `Vec<Setting>`; the renderer reads these directly.
    pub options: StatOptionsIr,
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
    /// Geometry-producing centroid stat (spec §15.13).
    Centroid,
    /// Geometry-producing Douglas–Peucker simplification (spec §15.13).
    Simplify,
    /// Spatial join of point geometries against a polygon table (spec §15.14).
    SpatialJoin,
}

impl StatKind {
    /// The authoritative display name for this stat, used in debug JSON and
    /// diagnostics.
    pub fn display_name(self) -> &'static str {
        match self {
            StatKind::Bin => "Bin",
            StatKind::Bin2D => "Bin2D",
            StatKind::HexBin => "HexBin",
            StatKind::Count => "Count",
            StatKind::Smooth => "Smooth",
            StatKind::Boxplot => "Boxplot",
            StatKind::Density => "Density",
            StatKind::Centroid => "Centroid",
            StatKind::Simplify => "Simplify",
            StatKind::SpatialJoin => "SpatialJoin",
        }
    }
}

/// Typed options for a built-in statistical transform (spec §13.4). Each variant
/// carries the user-specified values; `None` means "use the renderer default".
/// Fixed-domain settings (`closed`, smooth `method`) are enums.
#[derive(Debug, Clone, PartialEq)]
pub enum StatOptionsIr {
    Bin {
        bins: Option<f64>,
        bin_width: Option<f64>,
        boundary: Option<f64>,
        closed: BinClosedIr,
        interval: Option<BinIntervalIr>,
    },
    Bin2D {
        bins: Option<f64>,
    },
    HexBin {
        bins: Option<f64>,
    },
    Smooth {
        method: SmoothMethodIr,
        /// Loess neighborhood fraction in `(0, 1]`; `None` uses the renderer
        /// default. Only meaningful for `loess`.
        span: Option<f64>,
        /// Emit `ymin`/`ymax`/`se` confidence-band columns (spec §15.x).
        se: bool,
    },
    Density {
        bandwidth: Option<f64>,
        grid_points: Option<f64>,
    },
    Count,
    /// Centroid takes no options.
    Centroid,
    /// Simplification tolerance in the geometry's coordinate units (degrees for
    /// WGS84); `None` uses the renderer default.
    Simplify {
        tolerance: Option<f64>,
    },
    /// Spatial join: append a named polygon table's attributes to each point by
    /// spatial predicate (spec §15.14).
    SpatialJoin {
        table: String,
        predicate: SpatialPredicateIr,
    },
}

/// The spatial predicate a [`StatKind::SpatialJoin`] matches on (spec §15.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialPredicateIr {
    /// The point lies within the polygon.
    Within,
}

/// Calendar-aware bin interval units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinIntervalIr {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl BinIntervalIr {
    pub fn as_str(self) -> &'static str {
        match self {
            BinIntervalIr::Minute => "minute",
            BinIntervalIr::Hour => "hour",
            BinIntervalIr::Day => "day",
            BinIntervalIr::Week => "week",
            BinIntervalIr::Month => "month",
            BinIntervalIr::Quarter => "quarter",
            BinIntervalIr::Year => "year",
        }
    }
}

/// Which side of a histogram bin interval is closed (spec §15.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinClosedIr {
    #[default]
    Left,
    Right,
}

impl BinClosedIr {
    /// The authoritative source spelling (`"left"` / `"right"`).
    pub fn as_str(self) -> &'static str {
        match self {
            BinClosedIr::Left => "left",
            BinClosedIr::Right => "right",
        }
    }
}

/// The smoothing method for a `Smooth` stat (spec §15.x): ordinary
/// linear-model fitting or locally weighted regression (loess).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmoothMethodIr {
    #[default]
    Lm,
    Loess,
}

impl SmoothMethodIr {
    /// The authoritative source spelling (`"lm"` / `"loess"`).
    pub fn as_str(self) -> &'static str {
        match self {
            SmoothMethodIr::Lm => "lm",
            SmoothMethodIr::Loess => "loess",
        }
    }
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
    /// The cartographic projection for a spatial space (spec §16.14): a friendly
    /// alias (e.g. `"albers_usa"`) or a raw `+proj=…` PROJ string. `None` leaves
    /// the space non-spatial unless its frame is a geometry column, in which
    /// case the default equirectangular projection applies.
    pub projection: Option<String>,
    /// The coordinate system for this space (spec §4.2, §16.16). Cartesian is the
    /// default; `coords: "polar"` remaps scale ranges into a polar frame.
    pub coords: CoordsIr,
    pub span: Span,
}

/// The coordinate system of a space (spec §4.2, §16.16). Cartesian is implicit
/// and unchanged from earlier versions; polar is opt-in via `coords: "polar"`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CoordsIr {
    #[default]
    Cartesian,
    /// A polar transform: one frame axis maps to the angle, the other to the
    /// radius. `inner_radius` is a fraction of the maximum radius in `[0, 1)`
    /// (`0` = pie, `> 0` = donut).
    Polar {
        theta: PolarThetaIr,
        inner_radius: f64,
    },
}

/// Which frame axis maps to the angle under a polar transform (spec §16.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PolarThetaIr {
    /// The x (first) frame axis maps to the angle; y maps to radius. Default.
    #[default]
    X,
    /// The y (second) frame axis maps to the angle; x maps to radius.
    Y,
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
    /// Polymorphic spatial mark: dispatches on each row's geometry value
    /// (Point→circle, LineString→polyline, Polygon/MultiPolygon→path),
    /// projecting coordinates through the spatial scale (spec §14.x).
    Geo,
    /// Spatial-only guide mark: draws longitude/latitude grid lines projected
    /// through the active spatial scale (spec §14.24).
    Graticule,
}

impl GeometryKind {
    /// Human-facing geometry name, used in diagnostics and debug JSON. This is
    /// the single authoritative spelling, reused by the geometry registry.
    pub const fn display_name(self) -> &'static str {
        match self {
            GeometryKind::Point => "Point",
            GeometryKind::Line => "Line",
            GeometryKind::Path => "Path",
            GeometryKind::Bar => "Bar",
            GeometryKind::Rect => "Rect",
            GeometryKind::Histogram => "Histogram",
            GeometryKind::FreqPoly => "FreqPoly",
            GeometryKind::Bin2D => "Bin2D",
            GeometryKind::HexBin => "HexBin",
            GeometryKind::Smooth => "Smooth",
            GeometryKind::Boxplot => "Boxplot",
            GeometryKind::Violin => "Violin",
            GeometryKind::Density => "Density",
            GeometryKind::Ribbon => "Ribbon",
            GeometryKind::Tile => "Tile",
            GeometryKind::HLine => "HLine",
            GeometryKind::VLine => "VLine",
            GeometryKind::Rug => "Rug",
            GeometryKind::Area => "Area",
            GeometryKind::Text => "Text",
            GeometryKind::Segment => "Segment",
            GeometryKind::Geo => "Geo",
            GeometryKind::Graticule => "Graticule",
        }
    }

    /// Stable CSS suffix for rendered geometry layer classes.
    pub fn css_class(self) -> &'static str {
        match self {
            GeometryKind::Point => "point",
            GeometryKind::Line => "line",
            GeometryKind::Path => "path",
            GeometryKind::Bar => "bar",
            GeometryKind::Rect => "rect",
            GeometryKind::Histogram => "histogram",
            GeometryKind::FreqPoly => "freqpoly",
            GeometryKind::Bin2D => "bin2d",
            GeometryKind::HexBin => "hexbin",
            GeometryKind::Smooth => "smooth",
            GeometryKind::Boxplot => "boxplot",
            GeometryKind::Violin => "violin",
            GeometryKind::Density => "density",
            GeometryKind::Ribbon => "ribbon",
            GeometryKind::Tile => "tile",
            GeometryKind::HLine => "hline",
            GeometryKind::VLine => "vline",
            GeometryKind::Rug => "rug",
            GeometryKind::Area => "area",
            GeometryKind::Text => "text",
            GeometryKind::Segment => "segment",
            GeometryKind::Geo => "geo",
            GeometryKind::Graticule => "graticule",
        }
    }
}

/// A built-in geometry property or aesthetic key (spec §13.9).
///
/// Every property the geometry registry accepts has exactly one `PropertyKey`
/// variant, so the renderer and lowering match on variants instead of comparing
/// strings (spec §13.9). [`PropertyKey::as_str`] is the single authoritative
/// spelling, shared by the registry, diagnostics, and debug JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyKey {
    Fill,
    Stroke,
    StrokeWidth,
    Dash,
    Alpha,
    Size,
    Shape,
    Group,
    Layout,
    Stat,
    Bins,
    BinWidth,
    Boundary,
    Closed,
    Interval,
    Method,
    Span,
    Se,
    Bandwidth,
    N,
    Quantiles,
    Outliers,
    Width,
    Baseline,
    Label,
    Anchor,
    Dx,
    Dy,
    Declutter,
    Taper,
    Sides,
    X,
    Y,
    Xmin,
    Xmax,
    Ymin,
    Ymax,
    Xend,
    Yend,
    Step,
}

/// Every [`PropertyKey`] variant, in declaration order. Used by registry
/// round-trip checks and exhaustive iteration.
pub const PROPERTY_KEYS: &[PropertyKey] = &[
    PropertyKey::Fill,
    PropertyKey::Stroke,
    PropertyKey::StrokeWidth,
    PropertyKey::Dash,
    PropertyKey::Alpha,
    PropertyKey::Size,
    PropertyKey::Shape,
    PropertyKey::Group,
    PropertyKey::Layout,
    PropertyKey::Stat,
    PropertyKey::Bins,
    PropertyKey::BinWidth,
    PropertyKey::Boundary,
    PropertyKey::Closed,
    PropertyKey::Interval,
    PropertyKey::Method,
    PropertyKey::Span,
    PropertyKey::Se,
    PropertyKey::Bandwidth,
    PropertyKey::N,
    PropertyKey::Quantiles,
    PropertyKey::Outliers,
    PropertyKey::Width,
    PropertyKey::Baseline,
    PropertyKey::Label,
    PropertyKey::Anchor,
    PropertyKey::Dx,
    PropertyKey::Dy,
    PropertyKey::Declutter,
    PropertyKey::Taper,
    PropertyKey::Sides,
    PropertyKey::X,
    PropertyKey::Y,
    PropertyKey::Xmin,
    PropertyKey::Xmax,
    PropertyKey::Ymin,
    PropertyKey::Ymax,
    PropertyKey::Xend,
    PropertyKey::Yend,
    PropertyKey::Step,
];

impl PropertyKey {
    /// The single authoritative source spelling of this property key.
    pub const fn as_str(self) -> &'static str {
        match self {
            PropertyKey::Fill => "fill",
            PropertyKey::Stroke => "stroke",
            PropertyKey::StrokeWidth => "strokeWidth",
            PropertyKey::Dash => "dash",
            PropertyKey::Alpha => "alpha",
            PropertyKey::Size => "size",
            PropertyKey::Shape => "shape",
            PropertyKey::Group => "group",
            PropertyKey::Layout => "layout",
            PropertyKey::Stat => "stat",
            PropertyKey::Bins => "bins",
            PropertyKey::BinWidth => "binWidth",
            PropertyKey::Boundary => "boundary",
            PropertyKey::Closed => "closed",
            PropertyKey::Interval => "interval",
            PropertyKey::Method => "method",
            PropertyKey::Span => "span",
            PropertyKey::Se => "se",
            PropertyKey::Bandwidth => "bandwidth",
            PropertyKey::N => "n",
            PropertyKey::Quantiles => "quantiles",
            PropertyKey::Outliers => "outliers",
            PropertyKey::Width => "width",
            PropertyKey::Baseline => "baseline",
            PropertyKey::Label => "label",
            PropertyKey::Anchor => "anchor",
            PropertyKey::Dx => "dx",
            PropertyKey::Dy => "dy",
            PropertyKey::Declutter => "declutter",
            PropertyKey::Taper => "taper",
            PropertyKey::Sides => "sides",
            PropertyKey::X => "x",
            PropertyKey::Y => "y",
            PropertyKey::Xmin => "xmin",
            PropertyKey::Xmax => "xmax",
            PropertyKey::Ymin => "ymin",
            PropertyKey::Ymax => "ymax",
            PropertyKey::Xend => "xend",
            PropertyKey::Yend => "yend",
            PropertyKey::Step => "step",
        }
    }

    /// Resolve a registry property name to its typed key. Returns `None` for a
    /// name no built-in geometry property uses.
    pub fn from_name(name: &str) -> Option<PropertyKey> {
        PROPERTY_KEYS
            .iter()
            .copied()
            .find(|key| key.as_str() == name)
    }
}

impl std::fmt::Display for PropertyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A binding from an aesthetic to a data column (spec §13.6). `span` covers the
/// user-authored argument (or the originating geometry call for a mapping
/// synthesized during lowering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AestheticMapping {
    pub aesthetic: PropertyKey,
    pub column: ColumnRef,
    pub span: Span,
}

/// A geometry setting bound to a literal value (spec §13.6). `span` covers the
/// user-authored argument (or the originating geometry call for a setting
/// synthesized during lowering).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometrySetting {
    pub name: PropertyKey,
    pub value: SettingValue,
    pub span: Span,
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
