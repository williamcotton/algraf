//! Geometry and property registry (spec §13.8–13.9).
//!
//! The registry drives geometry-name validation, property validation, and
//! completion. Each geometry lists its accepted properties; each property lists
//! the value forms it accepts and whether it is required.

use crate::ir::GeometryKind;

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
