//! Geometry and property registry (spec §13.8–13.9).
//!
//! The registry drives geometry-name validation, property validation, and
//! completion. Each geometry lists its accepted properties; each property lists
//! the value forms it accepts and whether it is required.

use crate::ir::GeometryKind;

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
pub const SCALE_TYPE_NAMES: &[&str] = &["linear", "log10"];

/// Named categorical palettes accepted by `Scale(palette: ...)`.
pub const PALETTE_NAMES: &[&str] = &["default", "accent"];

/// The ordered argument names for a declaration keyword.
pub fn declaration_arg_names(decl: &str) -> &'static [&'static str] {
    match decl {
        "Layout" => &["facetColumns"],
        "Guide" => &["axis", "label", "legend", "fill", "stroke", "grid"],
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
            "label",
        ],
        _ => &[],
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

/// A geometry property definition (spec §13.9).
#[derive(Debug, Clone, Copy)]
pub struct PropSpec {
    pub name: &'static str,
    pub accepts: &'static [Accept],
    pub required: bool,
}

const fn opt(name: &'static str, accepts: &'static [Accept]) -> PropSpec {
    PropSpec {
        name,
        accepts,
        required: false,
    }
}

const fn req(name: &'static str, accepts: &'static [Accept]) -> PropSpec {
    PropSpec {
        name,
        accepts,
        required: true,
    }
}

/// A geometry definition (spec §13.8).
#[derive(Debug, Clone, Copy)]
pub struct GeometryDef {
    pub name: &'static str,
    pub kind: GeometryKind,
    pub props: &'static [PropSpec],
}

// Common aesthetic value forms.
const FILL: &[Accept] = &[Accept::Column, Accept::Color];
const STROKE: &[Accept] = &[Accept::Column, Accept::Color];
const ALPHA: &[Accept] = &[Accept::Number, Accept::Column];
const SIZE: &[Accept] = &[Accept::Number, Accept::Column];
const SHAPE: &[Accept] = &[Accept::Column, Accept::Str];
const STROKE_WIDTH: &[Accept] = &[Accept::Number];
/// `strokeWidth` for `Line`/`Path`, which support a data-driven (per-segment)
/// width in addition to a constant line width (spec §13.8).
const LINE_STROKE_WIDTH: &[Accept] = &[Accept::Number, Accept::Column];
const POS: &[Accept] = &[Accept::Column, Accept::Number];
const GROUP: &[Accept] = &[Accept::Column];

const POINT: &[PropSpec] = &[
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("alpha", ALPHA),
    opt("size", SIZE),
    opt("shape", SHAPE),
];

const LINE: &[PropSpec] = &[
    opt("stroke", STROKE),
    opt("strokeWidth", LINE_STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("group", GROUP),
];

const PATH: &[PropSpec] = &[
    opt("stroke", STROKE),
    opt("strokeWidth", LINE_STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("group", GROUP),
];

const BAR: &[PropSpec] = &[
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("layout", &[Accept::Enum(&["identity", "stack", "fill"])]),
    opt("stat", &[Accept::Enum(&["identity", "count"])]),
];

const RECT: &[PropSpec] = &[
    req("xmin", POS),
    req("xmax", POS),
    req("ymin", POS),
    req("ymax", POS),
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const HISTOGRAM: &[PropSpec] = &[
    opt("bins", &[Accept::Number]),
    opt("binWidth", &[Accept::Number]),
    opt("boundary", &[Accept::Number]),
    opt("closed", &[Accept::Enum(&["left", "right"])]),
    opt("fill", &[Accept::Color]),
    opt("stroke", &[Accept::Color]),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const FREQ_POLY: &[PropSpec] = &[
    opt("bins", &[Accept::Number]),
    opt("binWidth", &[Accept::Number]),
    opt("boundary", &[Accept::Number]),
    opt("closed", &[Accept::Enum(&["left", "right"])]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("group", GROUP),
];

const BIN2D: &[PropSpec] = &[
    opt("bins", &[Accept::Number]),
    opt("fill", &[Accept::Color]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const HEXBIN: &[PropSpec] = &[
    opt("bins", &[Accept::Number]),
    opt("fill", &[Accept::Color]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const SMOOTH: &[PropSpec] = &[
    opt("method", &[Accept::Enum(&["lm"])]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("group", GROUP),
];

const DENSITY: &[PropSpec] = &[
    opt("bandwidth", &[Accept::Number]),
    opt("n", &[Accept::Number]),
    opt("fill", &[Accept::Color]),
    opt("stroke", &[Accept::Color]),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const BOXPLOT: &[PropSpec] = &[
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("width", &[Accept::Number]),
];

const VIOLIN: &[PropSpec] = &[
    opt("bandwidth", &[Accept::Number]),
    opt("n", &[Accept::Number]),
    opt("quantiles", &[Accept::NumberArray]),
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("width", &[Accept::Number]),
];

const RIBBON: &[PropSpec] = &[
    req("ymin", POS),
    req("ymax", POS),
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const TILE: &[PropSpec] = &[
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const HLINE: &[PropSpec] = &[
    req("y", &[Accept::Number]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("label", &[Accept::Str]),
];

const VLINE: &[PropSpec] = &[
    req("x", &[Accept::Number]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
    opt("label", &[Accept::Str]),
];

const RUG: &[PropSpec] = &[
    opt("sides", &[Accept::Str]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const AREA: &[PropSpec] = &[
    opt("baseline", &[Accept::Number]),
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const TEXT: &[PropSpec] = &[
    req("label", &[Accept::Column, Accept::Str]),
    opt("fill", FILL),
    opt("alpha", ALPHA),
    opt("size", SIZE),
    opt("anchor", &[Accept::Enum(&["start", "middle", "end"])]),
    opt("dx", &[Accept::Column, Accept::Number]),
    opt("dy", &[Accept::Column, Accept::Number]),
    opt("declutter", &[Accept::Bool]),
];

const GEO: &[PropSpec] = &[
    opt("fill", FILL),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const SEGMENT: &[PropSpec] = &[
    req("x", &[Accept::Number]),
    req("y", &[Accept::Number]),
    req("xend", &[Accept::Number]),
    req("yend", &[Accept::Number]),
    opt("stroke", STROKE),
    opt("strokeWidth", STROKE_WIDTH),
    opt("alpha", ALPHA),
];

const GEOMETRIES: &[GeometryDef] = &[
    GeometryDef {
        name: "Point",
        kind: GeometryKind::Point,
        props: POINT,
    },
    GeometryDef {
        name: "Line",
        kind: GeometryKind::Line,
        props: LINE,
    },
    GeometryDef {
        name: "Path",
        kind: GeometryKind::Path,
        props: PATH,
    },
    GeometryDef {
        name: "Bar",
        kind: GeometryKind::Bar,
        props: BAR,
    },
    GeometryDef {
        name: "Rect",
        kind: GeometryKind::Rect,
        props: RECT,
    },
    GeometryDef {
        name: "Histogram",
        kind: GeometryKind::Histogram,
        props: HISTOGRAM,
    },
    GeometryDef {
        name: "FreqPoly",
        kind: GeometryKind::FreqPoly,
        props: FREQ_POLY,
    },
    GeometryDef {
        name: "Bin2D",
        kind: GeometryKind::Bin2D,
        props: BIN2D,
    },
    GeometryDef {
        name: "HexBin",
        kind: GeometryKind::HexBin,
        props: HEXBIN,
    },
    GeometryDef {
        name: "Smooth",
        kind: GeometryKind::Smooth,
        props: SMOOTH,
    },
    GeometryDef {
        name: "Boxplot",
        kind: GeometryKind::Boxplot,
        props: BOXPLOT,
    },
    GeometryDef {
        name: "Violin",
        kind: GeometryKind::Violin,
        props: VIOLIN,
    },
    GeometryDef {
        name: "Density",
        kind: GeometryKind::Density,
        props: DENSITY,
    },
    GeometryDef {
        name: "Ribbon",
        kind: GeometryKind::Ribbon,
        props: RIBBON,
    },
    GeometryDef {
        name: "Tile",
        kind: GeometryKind::Tile,
        props: TILE,
    },
    GeometryDef {
        name: "HLine",
        kind: GeometryKind::HLine,
        props: HLINE,
    },
    GeometryDef {
        name: "VLine",
        kind: GeometryKind::VLine,
        props: VLINE,
    },
    GeometryDef {
        name: "Rug",
        kind: GeometryKind::Rug,
        props: RUG,
    },
    GeometryDef {
        name: "Area",
        kind: GeometryKind::Area,
        props: AREA,
    },
    GeometryDef {
        name: "Text",
        kind: GeometryKind::Text,
        props: TEXT,
    },
    GeometryDef {
        name: "Segment",
        kind: GeometryKind::Segment,
        props: SEGMENT,
    },
    GeometryDef {
        name: "Geo",
        kind: GeometryKind::Geo,
        props: GEO,
    },
];

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
        "Ribbon" => "Draws a band between lower and upper y values.",
        "Tile" => "Draws heatmap-style tiles in a two-dimensional space.",
        "HLine" => "Draws a horizontal reference line.",
        "VLine" => "Draws a vertical reference line.",
        "Rug" => "Draws marginal tick marks for observations.",
        "Area" => "Draws a filled area from a baseline to y values.",
        "Text" => "Draws text labels in the inherited space.",
        "Segment" => "Draws explicit line segments between endpoints.",
        "Geo" => "Draws geometry values in a spatial space.",
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
        "alpha" => "Opacity setting or data column mapping.",
        "size" => "Point or text size setting or data column mapping.",
        "shape" => "Point shape setting or data column mapping.",
        "group" => "Series grouping column, independent from color aesthetics.",
        "layout" => "Bar collision layout: `\"identity\"`, `\"stack\"`, or `\"fill\"`.",
        "stat" => "Geometry statistic option.",
        "bins" => "Histogram bin count.",
        "binWidth" => "Histogram bin width.",
        "boundary" => "Histogram bin boundary.",
        "closed" => "Histogram interval closure: `\"left\"` or `\"right\"`.",
        "bandwidth" => "Kernel density bandwidth.",
        "n" => "Number of kernel density grid points.",
        "quantiles" => "Violin quantile line positions.",
        "gradient" => "Continuous color gradient stops.",
        "xmin" => "Rectangle minimum x boundary.",
        "xmax" => "Rectangle maximum x boundary.",
        "ymin" => "Lower y boundary.",
        "ymax" => "Upper y boundary.",
        "method" => "Smooth fitting method.",
        "width" => "Geometry width setting.",
        "baseline" => "Area or bar baseline.",
        "label" => "Text label or reference-line label.",
        "anchor" => "Text anchor: `\"start\"`, `\"middle\"`, or `\"end\"`.",
        "dx" => "Horizontal text offset, in pixels: a number or a column mapping.",
        "dy" => "Vertical text offset, in pixels: a number or a column mapping.",
        "declutter" => "Spread vertically-overlapping Text labels apart (boolean).",
        "x" => "X position.",
        "y" => "Y position.",
        "xend" => "Segment end x position.",
        "yend" => "Segment end y position.",
        "sides" => "Rug sides setting.",
        "marginTop" => "Minimum top plot margin in pixels (floor over the computed margin).",
        "marginRight" => "Minimum right plot margin in pixels (floor over the computed margin).",
        "marginBottom" => "Minimum bottom plot margin in pixels (floor over the computed margin).",
        "marginLeft" => "Minimum left plot margin in pixels (floor over the computed margin).",
        _ => "Algraf argument.",
    }
}
