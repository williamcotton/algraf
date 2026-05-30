//! Geometry and property registry (spec §13.8–13.9).
//!
//! The registry drives geometry-name validation, property validation, and
//! completion. Each geometry lists its accepted properties; each property lists
//! the value forms it accepts and whether it is required.

use crate::ir::{GeometryKind, PropertyKey};

/// Recognized `Chart(...)` arguments.
pub const CHART_ARGS: &[&str] = &[
    "data",
    "width",
    "height",
    "title",
    "subtitle",
    "caption",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
];

/// Recognized named base themes (spec §20.1).
pub const THEME_NAMES: &[&str] = &["minimal", "classic", "light", "dark", "void"];

/// Recognized `Theme(...)` override keys (spec §20.8).
pub const THEME_OVERRIDE_KEYS: &[&str] = &[
    "axisText",
    "gridMajor",
    "fontFamily",
    "fontSize",
    "titleSize",
    "pointSize",
    "lineWidth",
    "background",
    "plotBackground",
    "axisColor",
    "gridColor",
    "textColor",
    "grid",
    "axes",
];

/// Scale aesthetic targets accepted in `Scale(...)` declarations.
pub const SCALE_AESTHETIC_TARGETS: &[&str] = &["fill", "stroke", "size", "strokeWidth"];

/// Named scale types accepted by `Scale(type: ...)`.
pub const SCALE_TYPE_NAMES: &[&str] = &["linear", "log10", "sqrt"];

/// Named categorical palettes accepted by `Scale(palette: ...)`.
pub const PALETTE_NAMES: &[&str] = &["default", "accent"];

/// The ordered argument names for a declaration keyword.
pub fn declaration_arg_names(decl: &str) -> &'static [&'static str] {
    match decl {
        "Algraf" => &["version", "features"],
        "Layout" => &["facetColumns"],
        "Parse" => &[
            "table", "column", "as", "format", "formats", "unit", "timezone", "onError", "anchor",
        ],
        "Guide" => &[
            "axis",
            "label",
            "timeFormat",
            "tickLabelAngle",
            "legend",
            "fill",
            "stroke",
            "grid",
        ],
        "Theme" => {
            const THEME_ARGS: &[&str] = &[
                "name",
                "axisText",
                "gridMajor",
                "fontFamily",
                "fontSize",
                "titleSize",
                "pointSize",
                "lineWidth",
                "background",
                "plotBackground",
                "axisColor",
                "gridColor",
                "textColor",
                "grid",
                "axes",
            ];
            THEME_ARGS
        }
        "Scale" => &[
            "axis",
            "type",
            "domain",
            "reverse",
            "integer",
            "fill",
            "stroke",
            "size",
            "strokeWidth",
            "palette",
            "gradient",
            "range",
            "labels",
            "label",
        ],
        "Style" => &[
            "fill",
            "stroke",
            "strokeWidth",
            "alpha",
            "size",
            "shape",
            "group",
            "label",
            "dx",
            "dy",
        ],
        "Stop" => &["value", "color"],
        "Bin" => &["bins", "binWidth", "boundary", "closed", "interval"],
        "Smooth" => &["method", "span", "se"],
        "StepVertices" => &["direction"],
        "VectorEndpoints" => &["lengthScale"],
        "CurveSample" => &["curvature", "points"],
        "IntervalSegments" => &["orientation", "capWidth"],
        "IntervalRects" => &["orientation", "width"],
        "IntervalMiddles" => &["orientation", "width"],
        "Simplify" => &["tolerance"],
        "SpatialJoin" => &["table", "predicate"],
        _ => &[],
    }
}

/// Human-facing stat documentation shared by editor completion and hover.
pub fn stat_doc(name: &str) -> &'static str {
    match name {
        "Bin" => "Derives one-dimensional bin boundaries and counts.",
        "Smooth" => "Derives fitted x/y rows for linear or loess smoothing.",
        "Bin2D" => "Derives rectangular two-dimensional bins.",
        "HexBin" => "Derives hexagonal two-dimensional bins.",
        "StepVertices" => "Expands source x/y rows into orthogonal Path vertices.",
        "VectorEndpoints" => {
            "Computes Segment endpoint columns from x/y, angle in radians, and length."
        }
        "CurveSample" => "Samples grouped Path vertices for one quadratic curve per row.",
        "IntervalSegments" => {
            "Derives primitive Segment endpoint rows for vertical or horizontal intervals."
        }
        "IntervalRects" => "Derives primitive Rect bounds for interval bodies.",
        "IntervalMiddles" => "Derives primitive Segment endpoint rows for interval middle lines.",
        "Centroid" => "Derives centroid geometries from a geometry column.",
        "Simplify" => "Derives simplified geometries from a geometry column.",
        "SpatialJoin" => "Joins point geometries to a chart-scoped polygon table.",
        _ => "Algraf stat.",
    }
}

/// A value form a property accepts (spec §13.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// A column mapping (bare or quoted identifier).
    Column,
    /// A numeric literal.
    Number,
    /// A color string literal.
    Color,
    /// A free string literal (e.g. an axis label).
    Str,
    /// A boolean literal.
    Bool,
    /// One of a fixed set of string-literal enum values.
    Enum(&'static [&'static str]),
    /// An array of numeric literals.
    NumberArray,
}

/// A geometry property definition (spec §13.9). `key` is the typed property
/// identity; `name` is its authoritative source spelling, derived from `key` so
/// the registry never duplicates a property name (spec §13.9).
#[derive(Debug, Clone, Copy)]
pub struct PropSpec {
    pub key: PropertyKey,
    pub name: &'static str,
    pub accepts: &'static [Accept],
    pub required: bool,
}

const fn opt(key: PropertyKey, accepts: &'static [Accept]) -> PropSpec {
    PropSpec {
        key,
        name: key.as_str(),
        accepts,
        required: false,
    }
}

const fn req(key: PropertyKey, accepts: &'static [Accept]) -> PropSpec {
    PropSpec {
        key,
        name: key.as_str(),
        accepts,
        required: true,
    }
}

/// A geometry definition (spec §13.8). `name` is derived from
/// [`GeometryKind::display_name`] so the registry never duplicates a geometry's
/// authoritative spelling.
#[derive(Debug, Clone, Copy)]
pub struct GeometryDef {
    pub name: &'static str,
    pub kind: GeometryKind,
    pub props: &'static [PropSpec],
}

const fn geo(kind: GeometryKind, props: &'static [PropSpec]) -> GeometryDef {
    GeometryDef {
        name: kind.display_name(),
        kind,
        props,
    }
}

// Common aesthetic value forms.
const FILL: &[Accept] = &[Accept::Column, Accept::Color];
const STROKE: &[Accept] = &[Accept::Column, Accept::Color];
const ALPHA: &[Accept] = &[Accept::Number, Accept::Column];
const SIZE: &[Accept] = &[Accept::Number, Accept::Column];
const SHAPE: &[Accept] = &[Accept::Column, Accept::Str];
const STROKE_WIDTH: &[Accept] = &[Accept::Number];
const DASH: &[Accept] = &[Accept::Enum(&["solid", "dotted", "dashed"])];
const INTERVAL_ORIENTATION: &[Accept] = &[Accept::Enum(&["vertical", "horizontal"])];
/// `strokeWidth` for `Line`/`Path`, which support a data-driven (per-segment)
/// width in addition to a constant line width (spec §13.8).
const LINE_STROKE_WIDTH: &[Accept] = &[Accept::Number, Accept::Column];
const POS: &[Accept] = &[Accept::Column, Accept::Number];
const GROUP: &[Accept] = &[Accept::Column];
const BIN_INTERVAL: &[Accept] = &[Accept::Enum(&[
    "minute", "hour", "day", "week", "month", "quarter", "year",
])];

const POINT: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Size, SIZE),
    opt(PropertyKey::Shape, SHAPE),
];

const LINE: &[PropSpec] = &[
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, LINE_STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Group, GROUP),
    opt(PropertyKey::Taper, &[Accept::Bool]),
];

const PATH: &[PropSpec] = &[
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, LINE_STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Group, GROUP),
    opt(PropertyKey::Taper, &[Accept::Bool]),
];

const BAR: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(
        PropertyKey::Layout,
        &[Accept::Enum(&["identity", "stack", "fill"])],
    ),
    opt(PropertyKey::Stat, &[Accept::Enum(&["identity", "count"])]),
    // A categorical `radius:` mapping selects concentric rings for the polar
    // `radial_bar` mode (spec §16.16); ignored for Cartesian bars.
    opt(PropertyKey::Radius, &[Accept::Column]),
];

const RECT: &[PropSpec] = &[
    req(PropertyKey::Xmin, POS),
    req(PropertyKey::Xmax, POS),
    req(PropertyKey::Ymin, POS),
    req(PropertyKey::Ymax, POS),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const HISTOGRAM: &[PropSpec] = &[
    opt(PropertyKey::Bins, &[Accept::Number]),
    opt(PropertyKey::BinWidth, &[Accept::Number]),
    opt(PropertyKey::Boundary, &[Accept::Number]),
    opt(PropertyKey::Closed, &[Accept::Enum(&["left", "right"])]),
    opt(PropertyKey::Interval, BIN_INTERVAL),
    // A `fill` column groups the histogram (stacked); a color fills a single
    // series (spec §15.6).
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, &[Accept::Color]),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Group, GROUP),
];

const FREQ_POLY: &[PropSpec] = &[
    opt(PropertyKey::Bins, &[Accept::Number]),
    opt(PropertyKey::BinWidth, &[Accept::Number]),
    opt(PropertyKey::Boundary, &[Accept::Number]),
    opt(PropertyKey::Closed, &[Accept::Enum(&["left", "right"])]),
    opt(PropertyKey::Interval, BIN_INTERVAL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Group, GROUP),
];

const BIN2D: &[PropSpec] = &[
    opt(PropertyKey::Bins, &[Accept::Number]),
    opt(PropertyKey::Fill, &[Accept::Color]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const HEXBIN: &[PropSpec] = &[
    opt(PropertyKey::Bins, &[Accept::Number]),
    opt(PropertyKey::Fill, &[Accept::Color]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const SMOOTH: &[PropSpec] = &[
    opt(PropertyKey::Method, &[Accept::Enum(&["lm", "loess"])]),
    opt(PropertyKey::Span, &[Accept::Number]),
    opt(PropertyKey::Se, &[Accept::Bool]),
    opt(PropertyKey::Fill, &[Accept::Color]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Group, GROUP),
];

const DENSITY: &[PropSpec] = &[
    opt(PropertyKey::Bandwidth, &[Accept::Number]),
    opt(PropertyKey::N, &[Accept::Number]),
    opt(PropertyKey::Fill, &[Accept::Color]),
    opt(PropertyKey::Stroke, &[Accept::Color]),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const ERROR_BAR: &[PropSpec] = &[
    opt(PropertyKey::Xmin, &[Accept::Column]),
    opt(PropertyKey::Xmax, &[Accept::Column]),
    opt(PropertyKey::Ymin, &[Accept::Column]),
    opt(PropertyKey::Ymax, &[Accept::Column]),
    opt(PropertyKey::Orientation, INTERVAL_ORIENTATION),
    opt(PropertyKey::CapWidth, &[Accept::Number]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
];

const LINE_RANGE: &[PropSpec] = &[
    opt(PropertyKey::Xmin, &[Accept::Column]),
    opt(PropertyKey::Xmax, &[Accept::Column]),
    opt(PropertyKey::Ymin, &[Accept::Column]),
    opt(PropertyKey::Ymax, &[Accept::Column]),
    opt(PropertyKey::Orientation, INTERVAL_ORIENTATION),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
];

const POINT_RANGE: &[PropSpec] = &[
    opt(PropertyKey::Xmin, &[Accept::Column]),
    opt(PropertyKey::Xmax, &[Accept::Column]),
    opt(PropertyKey::Ymin, &[Accept::Column]),
    opt(PropertyKey::Ymax, &[Accept::Column]),
    opt(PropertyKey::Orientation, INTERVAL_ORIENTATION),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Size, SIZE),
    opt(PropertyKey::Shape, SHAPE),
];

const CROSS_BAR: &[PropSpec] = &[
    opt(PropertyKey::Xmin, &[Accept::Column]),
    opt(PropertyKey::Xmax, &[Accept::Column]),
    opt(PropertyKey::Ymin, &[Accept::Column]),
    opt(PropertyKey::Ymax, &[Accept::Column]),
    opt(PropertyKey::Orientation, INTERVAL_ORIENTATION),
    opt(PropertyKey::Width, &[Accept::Number]),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
];

const BOXPLOT: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Width, &[Accept::Number]),
    opt(PropertyKey::Outliers, &[Accept::Bool]),
];

const VIOLIN: &[PropSpec] = &[
    opt(PropertyKey::Bandwidth, &[Accept::Number]),
    opt(PropertyKey::N, &[Accept::Number]),
    opt(PropertyKey::Quantiles, &[Accept::NumberArray]),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Width, &[Accept::Number]),
];

const RIBBON: &[PropSpec] = &[
    req(PropertyKey::Ymin, POS),
    req(PropertyKey::Ymax, POS),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const TILE: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const HLINE: &[PropSpec] = &[
    req(PropertyKey::Y, &[Accept::Number]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Label, &[Accept::Str]),
];

const VLINE: &[PropSpec] = &[
    req(PropertyKey::X, &[Accept::Number]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Label, &[Accept::Str]),
];

const RUG: &[PropSpec] = &[
    opt(PropertyKey::Sides, &[Accept::Str]),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const AREA: &[PropSpec] = &[
    opt(PropertyKey::Baseline, &[Accept::Number]),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const TEXT: &[PropSpec] = &[
    req(PropertyKey::Label, &[Accept::Column, Accept::Str]),
    opt(PropertyKey::X, POS),
    opt(PropertyKey::Y, POS),
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Size, SIZE),
    opt(
        PropertyKey::Anchor,
        &[Accept::Enum(&["start", "middle", "end"])],
    ),
    opt(PropertyKey::Dx, &[Accept::Column, Accept::Number]),
    opt(PropertyKey::Dy, &[Accept::Column, Accept::Number]),
    opt(PropertyKey::Declutter, &[Accept::Bool]),
    // Format a temporal `label:` column using the §19.4 named/custom model.
    opt(PropertyKey::TimeFormat, &[Accept::Str]),
];

const GEO: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
];

const GRATICULE: &[PropSpec] = &[
    opt(PropertyKey::Stroke, &[Accept::Color]),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Step, &[Accept::Number]),
];

const SEGMENT: &[PropSpec] = &[
    req(PropertyKey::X, POS),
    req(PropertyKey::Y, POS),
    req(PropertyKey::Xend, POS),
    req(PropertyKey::Yend, POS),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::StrokeWidth, STROKE_WIDTH),
    opt(PropertyKey::Dash, DASH),
    opt(PropertyKey::Alpha, ALPHA),
];

const GEOMETRIES: &[GeometryDef] = &[
    geo(GeometryKind::Point, POINT),
    geo(GeometryKind::Line, LINE),
    geo(GeometryKind::Path, PATH),
    geo(GeometryKind::Bar, BAR),
    geo(GeometryKind::Rect, RECT),
    geo(GeometryKind::Histogram, HISTOGRAM),
    geo(GeometryKind::FreqPoly, FREQ_POLY),
    geo(GeometryKind::Bin2D, BIN2D),
    geo(GeometryKind::HexBin, HEXBIN),
    geo(GeometryKind::Smooth, SMOOTH),
    geo(GeometryKind::Boxplot, BOXPLOT),
    geo(GeometryKind::Violin, VIOLIN),
    geo(GeometryKind::Density, DENSITY),
    geo(GeometryKind::ErrorBar, ERROR_BAR),
    geo(GeometryKind::LineRange, LINE_RANGE),
    geo(GeometryKind::PointRange, POINT_RANGE),
    geo(GeometryKind::CrossBar, CROSS_BAR),
    geo(GeometryKind::Ribbon, RIBBON),
    geo(GeometryKind::Tile, TILE),
    geo(GeometryKind::HLine, HLINE),
    geo(GeometryKind::VLine, VLINE),
    geo(GeometryKind::Rug, RUG),
    geo(GeometryKind::Area, AREA),
    geo(GeometryKind::Text, TEXT),
    geo(GeometryKind::Segment, SEGMENT),
    geo(GeometryKind::Geo, GEO),
    geo(GeometryKind::Graticule, GRATICULE),
];

/// Declarative interaction property names accepted on geometries that support
/// them (spec §14.25). These are not in any geometry's `PropSpec` list because
/// they carry a distinct value shape (a column, an array of columns, or a
/// grouping key) handled directly during analysis.
pub const INTERACTION_PROPS: &[&str] = &["tooltip", "highlight"];

/// Whether a geometry accepts declarative interaction metadata (`tooltip`,
/// `highlight`; spec §14.25, §24.6).
///
/// Interaction metadata attaches to one filled mark per datum, so a per-mark
/// accessible `<title>` and highlight group attach cleanly. The supported set is
/// the per-datum filled marks: `Point`, `Bar`, `Rect`, and `Tile`.
pub fn supports_interaction(kind: GeometryKind) -> bool {
    matches!(
        kind,
        GeometryKind::Point | GeometryKind::Bar | GeometryKind::Rect | GeometryKind::Tile
    )
}

/// Look up a geometry definition by exact (case-sensitive) name.
pub fn geometry(name: &str) -> Option<&'static GeometryDef> {
    GEOMETRIES.iter().find(|g| g.name == name)
}

/// All known geometry names, for suggestions and completion.
pub fn geometry_names() -> impl Iterator<Item = &'static str> {
    GEOMETRIES.iter().map(|g| g.name)
}

impl GeometryDef {
    pub fn prop(&self, name: &str) -> Option<&'static PropSpec> {
        self.props.iter().find(|p| p.name == name)
    }

    pub fn prop_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.props.iter().map(|p| p.name)
    }
}

/// Human-facing geometry documentation shared by LSP completion and hover.
pub fn geometry_doc(name: &str) -> &'static str {
    match name {
        "Point" => "Draws one point per row in the inherited space.",
        "Line" => "Draws connected line segments through row coordinates.",
        "Path" => "Draws an ungrouped connected path through row coordinates.",
        "Bar" => "Draws bars in the inherited categorical or Cartesian space.",
        "Rect" => "Draws rectangles from explicit boundary properties.",
        "Histogram" => "Bins one continuous vector and draws count bars.",
        "FreqPoly" => "Bins one continuous vector and connects bin centers.",
        "Bin2D" => "Bins two continuous dimensions into rectangles.",
        "HexBin" => "Bins two continuous dimensions into hexagons.",
        "Smooth" => "Draws a fitted smooth line over a two-dimensional space.",
        "Boxplot" => "Draws distribution summaries for grouped values.",
        "Violin" => "Draws mirrored KDE distributions per category.",
        "Density" => "Draws a kernel density estimate over one continuous vector.",
        "ErrorBar" => "Lowers to IntervalSegments plus Segment rows with optional caps.",
        "LineRange" => "Lowers to IntervalSegments plus Segment rows without caps.",
        "PointRange" => "Lowers to IntervalSegments plus Segment, then Point.",
        "CrossBar" => "Lowers to IntervalRects plus Rect and IntervalMiddles plus Segment.",
        "Ribbon" => "Draws a band between lower and upper y values.",
        "Tile" => "Draws heatmap-style tiles in a two-dimensional space.",
        "HLine" => "Draws a horizontal reference line.",
        "VLine" => "Draws a vertical reference line.",
        "Rug" => "Draws marginal tick marks for observations.",
        "Area" => "Draws a filled area from a baseline to y values.",
        "Text" => "Draws text labels in the inherited space.",
        "Segment" => "Draws explicit line segments between endpoints.",
        "Geo" => "Draws geometry values in a spatial space.",
        "Graticule" => "Draws projected longitude/latitude grid lines in a spatial space.",
        _ => "Algraf geometry.",
    }
}

/// Human-facing property documentation shared by LSP completion, hover, and
/// signature help.
pub fn property_doc(name: &str) -> &'static str {
    match name {
        "fill" => "Fill color setting or data column mapping.",
        "stroke" => "Stroke color setting or data column mapping.",
        "strokeWidth" => "Stroke width numeric setting.",
        "dash" => "Line dash style: `\"solid\"`, `\"dotted\"`, or `\"dashed\"`.",
        "alpha" => "Opacity setting or data column mapping.",
        "size" => "Point or text size setting or data column mapping.",
        "shape" => "Point shape setting or data column mapping.",
        "group" => "Series grouping column, independent from color aesthetics.",
        "layout" => "Bar collision layout: `\"identity\"`, `\"stack\"`, or `\"fill\"`.",
        "radius" => "Categorical column mapping selecting concentric rings for a polar radial bar chart (theta: \"y\").",
        "stat" => "Geometry statistic option.",
        "bins" => "Histogram bin count.",
        "binWidth" => "Histogram bin width.",
        "boundary" => "Histogram bin boundary.",
        "closed" => "Histogram interval closure: `\"left\"` or `\"right\"`.",
        "interval" => "Calendar interval for temporal bins.",
        "bandwidth" => "Kernel density bandwidth.",
        "n" => "Number of kernel density grid points.",
        "quantiles" => "Violin quantile line positions.",
        "outliers" => "Render Boxplot points beyond the 1.5·IQR whiskers (boolean, default true).",
        "orientation" => "Interval direction: `\"vertical\"` or `\"horizontal\"`.",
        "capWidth" => "ErrorBar cap width in position-axis data units.",
        "gradient" => "Continuous color gradient stops.",
        "style" => "Reusable `Style(...)` fragment applied at this argument position.",
        "timeFormat" => "Temporal axis label format: `\"iso-date\"` or `\"iso-minute\"`.",
        "tickLabelAngle" => "Axis tick label rotation angle in degrees, from -90 to 90.",
        "features" => "Source feature gates reserved for future language capabilities.",
        "version" => "Algraf source language version.",
        "xmin" => "Rectangle minimum x boundary.",
        "xmax" => "Rectangle maximum x boundary.",
        "ymin" => "Lower y boundary.",
        "ymax" => "Upper y boundary.",
        "method" => "Smooth fitting method: `\"lm\"` (linear) or `\"loess\"` (local regression).",
        "span" => "Loess neighborhood fraction in (0, 1]; larger values are smoother.",
        "se" => "Draw a confidence band around the smooth (boolean).",
        "curvature" => "CurveSample bend amount; negative values bend the opposite way.",
        "points" => "CurveSample vertices per source row, from 2 to 1024.",
        "lengthScale" => "Scale factor applied to VectorEndpoints lengths.",
        "width" => "Geometry width setting.",
        "baseline" => "Area or bar baseline.",
        "label" => "Text label or reference-line label.",
        "anchor" => "Text anchor: `\"start\"`, `\"middle\"`, or `\"end\"`.",
        "dx" => "Horizontal text offset, in pixels: a number or a column mapping.",
        "dy" => "Vertical text offset, in pixels: a number or a column mapping.",
        "declutter" => "Spread vertically-overlapping Text labels apart (boolean).",
        "taper" => {
            "Render a Line/Path with mapped strokeWidth as a filled tapered ribbon (boolean)."
        }
        "x" => "X position.",
        "y" => "Y position.",
        "xend" => "Segment end x position.",
        "yend" => "Segment end y position.",
        "sides" => "Rug sides setting.",
        "step" => {
            "Graticule line spacing in degrees (defaults to a value chosen from the map extent)."
        }
        "marginTop" => "Minimum top plot margin in pixels (floor over the computed margin).",
        "marginRight" => "Minimum right plot margin in pixels (floor over the computed margin).",
        "marginBottom" => "Minimum bottom plot margin in pixels (floor over the computed margin).",
        "marginLeft" => "Minimum left plot margin in pixels (floor over the computed margin).",
        "tooltip" => {
            "Declarative tooltip: a column or array of columns whose per-row values describe a mark. Inert metadata — never script."
        }
        "highlight" => {
            "Declarative highlight grouping key: a column whose value identifies marks that emphasize together on hover."
        }
        _ => "Algraf argument.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GeometryKind, GEOMETRY_KINDS, PROPERTY_KEYS};

    #[test]
    fn every_geometry_kind_has_one_matching_registry_entry() {
        assert_eq!(GEOMETRIES.len(), GEOMETRY_KINDS.len());

        let mut names = std::collections::HashSet::new();
        let mut css_classes = std::collections::HashSet::new();
        for &kind in GEOMETRY_KINDS {
            let name = kind.display_name();
            assert_eq!(GeometryKind::from_name(name), Some(kind));
            let def = geometry(name).unwrap_or_else(|| panic!("{name} missing from registry"));
            assert_eq!(def.name, name);
            assert_eq!(def.kind, kind);
            assert!(names.insert(name), "duplicate geometry name {name}");
            assert!(
                css_classes.insert(kind.css_class()),
                "duplicate geometry CSS class {}",
                kind.css_class()
            );
        }
    }

    #[test]
    fn every_registry_property_resolves_to_its_typed_key() {
        // The registry derives `name` from `key.as_str()`, so a property's name
        // and typed key must always agree and round-trip through `from_name`.
        for geo in GEOMETRIES {
            for prop in geo.props {
                assert_eq!(prop.name, prop.key.as_str());
                assert_eq!(PropertyKey::from_name(prop.name), Some(prop.key));
            }
        }
    }

    #[test]
    fn property_key_as_str_round_trips() {
        for &key in PROPERTY_KEYS {
            assert_eq!(PropertyKey::from_name(key.as_str()), Some(key));
        }
    }

    #[test]
    fn property_key_spellings_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for &key in PROPERTY_KEYS {
            assert!(seen.insert(key.as_str()), "duplicate spelling {key:?}");
        }
    }

    #[test]
    fn every_property_key_is_registered_or_builtin_special() {
        let mut seen = std::collections::HashSet::new();
        for geo in GEOMETRIES {
            for prop in geo.props {
                seen.insert(prop.key);
            }
        }
        for &key in PROPERTY_KEYS {
            assert!(
                seen.contains(&key),
                "{} is a PropertyKey but no geometry registry entry accepts it",
                key.as_str()
            );
        }
    }
}
