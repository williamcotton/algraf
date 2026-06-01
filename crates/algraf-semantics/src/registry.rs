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
    "alt",
    "description",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
];

/// Recognized named base themes (spec §20.1).
pub const THEME_NAMES: &[&str] = &[
    "minimal", "classic", "light", "dark", "void", "gray", "bw", "linedraw",
];

/// Recognized `Theme(...)` override keys (spec §20.8).
pub const THEME_OVERRIDE_KEYS: &[&str] = &[
    "axisText",
    "axisTitle",
    "plotTitle",
    "plotSubtitle",
    "plotCaption",
    "stripText",
    "legendTitle",
    "legendText",
    "panelBackground",
    "gridMajor",
    "gridMinor",
    "legendPosition",
    "legendSpacing",
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

/// Documentation for a single call argument or property.
#[derive(Debug, Clone, Copy)]
pub struct ArgDoc {
    pub name: &'static str,
    pub value: &'static str,
    pub default: Option<&'static str>,
    pub doc: &'static str,
}

/// Documentation for a declaration-style call.
#[derive(Debug, Clone, Copy)]
pub struct CallDoc {
    pub name: &'static str,
    pub kind: &'static str,
    pub description: &'static str,
    pub args: &'static [ArgDoc],
    pub example: &'static str,
}

const CHART_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "data",
        value: "source",
        default: None,
        doc: "Primary data source path, `stdin`, or a source constructor.",
    },
    ArgDoc {
        name: "width",
        value: "number",
        default: Some("800"),
        doc: "Viewport width in pixels.",
    },
    ArgDoc {
        name: "height",
        value: "number",
        default: Some("520"),
        doc: "Viewport height in pixels.",
    },
    ArgDoc {
        name: "title",
        value: "string",
        default: None,
        doc: "Main chart title.",
    },
    ArgDoc {
        name: "subtitle",
        value: "string",
        default: None,
        doc: "Secondary title text.",
    },
    ArgDoc {
        name: "caption",
        value: "string",
        default: None,
        doc: "Caption below the plot.",
    },
    ArgDoc {
        name: "alt",
        value: "string",
        default: None,
        doc: "Accessible short text label for the chart.",
    },
    ArgDoc {
        name: "description",
        value: "string",
        default: None,
        doc: "Accessible long description for the chart.",
    },
    ArgDoc {
        name: "marginTop",
        value: "number",
        default: Some("computed"),
        doc: "Minimum top plot margin in pixels.",
    },
    ArgDoc {
        name: "marginRight",
        value: "number",
        default: Some("computed"),
        doc: "Minimum right plot margin in pixels.",
    },
    ArgDoc {
        name: "marginBottom",
        value: "number",
        default: Some("computed"),
        doc: "Minimum bottom plot margin in pixels.",
    },
    ArgDoc {
        name: "marginLeft",
        value: "number",
        default: Some("computed"),
        doc: "Minimum left plot margin in pixels.",
    },
];

const SPACE_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "frame",
        value: "algebra",
        default: None,
        doc: "Positional algebra expression such as `x * y` or `category / group`.",
    },
    ArgDoc {
        name: "data",
        value: "table name",
        default: Some("primary"),
        doc: "Bind the space to a chart-scoped `Table` or `Derive` result.",
    },
    ArgDoc {
        name: "coords",
        value: "\"cartesian\" | \"polar\"",
        default: Some("\"cartesian\""),
        doc: "Coordinate system for the space.",
    },
    ArgDoc {
        name: "theta",
        value: "\"x\" | \"y\"",
        default: Some("\"x\""),
        doc: "Polar axis mapped to angle.",
    },
    ArgDoc {
        name: "innerRadius",
        value: "number",
        default: Some("0"),
        doc: "Polar inner-radius fraction in `[0, 1)`.",
    },
    ArgDoc {
        name: "startAngle",
        value: "number",
        default: Some("0"),
        doc: "Polar start angle in degrees.",
    },
    ArgDoc {
        name: "direction",
        value: "\"clockwise\" | \"counterclockwise\"",
        default: Some("\"clockwise\""),
        doc: "Polar sweep direction.",
    },
    ArgDoc {
        name: "projection",
        value: "string",
        default: None,
        doc: "Spatial projection alias or PROJ string.",
    },
    ArgDoc {
        name: "zoomX",
        value: "[number | null, number | null]",
        default: None,
        doc: "Visual x-axis zoom without pre-stat data filtering.",
    },
    ArgDoc {
        name: "zoomY",
        value: "[number | null, number | null]",
        default: None,
        doc: "Visual y-axis zoom without pre-stat data filtering.",
    },
    ArgDoc {
        name: "aspect",
        value: "number",
        default: None,
        doc: "Fixed Cartesian x/y unit aspect ratio.",
    },
];

const THEME_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "name",
        value: "\"minimal\" | \"classic\" | \"light\" | \"dark\" | \"void\" | \"gray\" | \"bw\" | \"linedraw\"",
        default: Some("inherited"),
        doc: "Named base theme.",
    },
    ArgDoc {
        name: "plotTitle",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Main chart title text style.",
    },
    ArgDoc {
        name: "plotSubtitle",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Chart subtitle text style.",
    },
    ArgDoc {
        name: "plotCaption",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Chart caption text style.",
    },
    ArgDoc {
        name: "axisTitle",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Axis title text style.",
    },
    ArgDoc {
        name: "axisText",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Axis tick-label text style.",
    },
    ArgDoc {
        name: "stripText",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Facet strip label text style.",
    },
    ArgDoc {
        name: "legendTitle",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Legend title text style.",
    },
    ArgDoc {
        name: "legendText",
        value: "Text(size?, fill?, fontFamily?)",
        default: None,
        doc: "Legend entry text style.",
    },
    ArgDoc {
        name: "panelBackground",
        value: "Rect(fill?, stroke?, strokeWidth?)",
        default: None,
        doc: "Plot panel background style.",
    },
    ArgDoc {
        name: "gridMajor",
        value: "Line(stroke?, strokeWidth?)",
        default: None,
        doc: "Major grid-line style.",
    },
    ArgDoc {
        name: "gridMinor",
        value: "Line(stroke?, strokeWidth?)",
        default: None,
        doc: "Minor grid-line style.",
    },
    ArgDoc {
        name: "legendPosition",
        value: "\"right\" | \"bottom\" | \"top\" | \"left\"",
        default: Some("\"right\""),
        doc: "Legend placement outside the plot panel.",
    },
    ArgDoc {
        name: "legendSpacing",
        value: "number",
        default: None,
        doc: "Spacing between legend blocks in pixels.",
    },
    ArgDoc {
        name: "fontFamily",
        value: "string",
        default: None,
        doc: "Font family used for chart text.",
    },
    ArgDoc {
        name: "fontSize",
        value: "number",
        default: None,
        doc: "Base text size in pixels.",
    },
    ArgDoc {
        name: "titleSize",
        value: "number",
        default: None,
        doc: "Title text size in pixels.",
    },
    ArgDoc {
        name: "pointSize",
        value: "number",
        default: None,
        doc: "Default point radius.",
    },
    ArgDoc {
        name: "lineWidth",
        value: "number",
        default: None,
        doc: "Default line width.",
    },
    ArgDoc {
        name: "background",
        value: "color",
        default: None,
        doc: "Chart background color.",
    },
    ArgDoc {
        name: "plotBackground",
        value: "color",
        default: None,
        doc: "Plot area background color.",
    },
    ArgDoc {
        name: "axisColor",
        value: "color",
        default: None,
        doc: "Axis stroke color.",
    },
    ArgDoc {
        name: "gridColor",
        value: "color",
        default: None,
        doc: "Grid-line color.",
    },
    ArgDoc {
        name: "textColor",
        value: "color",
        default: None,
        doc: "Default text color.",
    },
    ArgDoc {
        name: "grid",
        value: "boolean",
        default: None,
        doc: "Enable or disable grid lines.",
    },
    ArgDoc {
        name: "axes",
        value: "boolean",
        default: None,
        doc: "Enable or disable axes.",
    },
];

const SCALE_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "axis",
        value: "x | y",
        default: None,
        doc: "Axis scale target.",
    },
    ArgDoc {
        name: "type",
        value: "\"linear\" | \"log10\" | \"sqrt\"",
        default: Some("\"linear\""),
        doc: "Continuous scale transform.",
    },
    ArgDoc {
        name: "domain",
        value: "[number | null, number | null]",
        default: None,
        doc: "Explicit numeric domain bounds.",
    },
    ArgDoc {
        name: "mode",
        value: "\"binned\" | \"identity\"",
        default: None,
        doc: "Aesthetic scale mode for binned or identity color scales.",
    },
    ArgDoc {
        name: "breaks",
        value: "array",
        default: None,
        doc: "Exact axis ticks or legend entries.",
    },
    ArgDoc {
        name: "expand",
        value: "number | [number, number]",
        default: None,
        doc: "Domain expansion padding.",
    },
    ArgDoc {
        name: "reverse",
        value: "boolean",
        default: Some("false"),
        doc: "Reverse scale direction.",
    },
    ArgDoc {
        name: "integer",
        value: "boolean",
        default: Some("false"),
        doc: "Prefer whole-number axis ticks.",
    },
    ArgDoc {
        name: "fill",
        value: "column",
        default: None,
        doc: "Fill aesthetic target.",
    },
    ArgDoc {
        name: "stroke",
        value: "column",
        default: None,
        doc: "Stroke aesthetic target.",
    },
    ArgDoc {
        name: "size",
        value: "column",
        default: None,
        doc: "Size aesthetic target.",
    },
    ArgDoc {
        name: "strokeWidth",
        value: "column",
        default: None,
        doc: "Stroke-width aesthetic target.",
    },
    ArgDoc {
        name: "palette",
        value: "\"default\" | \"accent\"",
        default: Some("\"default\""),
        doc: "Categorical palette.",
    },
    ArgDoc {
        name: "gradient",
        value: "array",
        default: None,
        doc: "Continuous color gradient.",
    },
    ArgDoc {
        name: "range",
        value: "array | map",
        default: None,
        doc: "Output range or categorical color map.",
    },
    ArgDoc {
        name: "labels",
        value: "array | map",
        default: None,
        doc: "Manual break labels or category labels.",
    },
    ArgDoc {
        name: "label",
        value: "string",
        default: None,
        doc: "Legend title for an aesthetic scale.",
    },
];

const GUIDE_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "axis",
        value: "x | y",
        default: None,
        doc: "Axis targeted by guide settings.",
    },
    ArgDoc {
        name: "label",
        value: "string",
        default: None,
        doc: "Axis label override.",
    },
    ArgDoc {
        name: "timeFormat",
        value: "string",
        default: None,
        doc: "Temporal axis label format.",
    },
    ArgDoc {
        name: "tickLabelAngle",
        value: "number",
        default: Some("0"),
        doc: "Axis tick label angle in degrees.",
    },
    ArgDoc {
        name: "tickLabelRows",
        value: "number",
        default: Some("1"),
        doc: "Deterministic multi-row tick-label dodging.",
    },
    ArgDoc {
        name: "legend",
        value: "boolean",
        default: Some("true"),
        doc: "Enable or disable legends.",
    },
    ArgDoc {
        name: "fill",
        value: "null",
        default: None,
        doc: "Suppress the fill legend with `null`.",
    },
    ArgDoc {
        name: "stroke",
        value: "null",
        default: None,
        doc: "Suppress the stroke legend with `null`.",
    },
    ArgDoc {
        name: "grid",
        value: "boolean",
        default: Some("true"),
        doc: "Enable or disable grid lines.",
    },
];

const LAYOUT_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "facetColumns",
        value: "number",
        default: Some("auto"),
        doc: "Number of columns in a facet-wrap layout.",
    },
    ArgDoc {
        name: "facetRows",
        value: "column",
        default: None,
        doc: "Facet-grid row column.",
    },
    ArgDoc {
        name: "facetCols",
        value: "column",
        default: None,
        doc: "Facet-grid column column.",
    },
    ArgDoc {
        name: "facetScales",
        value: "\"fixed\" | \"free-x\" | \"free-y\" | \"free\"",
        default: Some("\"fixed\""),
        doc: "Facet panel scale-sharing mode.",
    },
    ArgDoc {
        name: "facetLabel",
        value: "\"value\" | \"name-value\"",
        default: Some("\"value\""),
        doc: "Facet strip label format.",
    },
    ArgDoc {
        name: "facetLabels",
        value: "map",
        default: None,
        doc: "Facet value label map.",
    },
    ArgDoc {
        name: "panelSpacing",
        value: "number | [number, number]",
        default: Some("theme"),
        doc: "Horizontal/vertical facet panel spacing in pixels.",
    },
];

const TABLE_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "name",
        value: "identifier",
        default: None,
        doc: "Chart-scoped table name.",
    },
    ArgDoc {
        name: "source",
        value: "source",
        default: None,
        doc: "Path or source constructor loaded like `Chart(data:)`.",
    },
];

const INSET_DOC_ARGS: &[ArgDoc] = &[
    ArgDoc {
        name: "data",
        value: "table name",
        default: None,
        doc: "Child table rendered inside each inset instance.",
    },
    ArgDoc {
        name: "match",
        value: "map",
        default: None,
        doc: "Explicit child-to-parent equi-match rules.",
    },
    ArgDoc {
        name: "size",
        value: "number | column",
        default: Some("32"),
        doc: "Square inset size or mapped parent column.",
    },
    ArgDoc {
        name: "width",
        value: "number",
        default: None,
        doc: "Fixed rectangular inset width.",
    },
    ArgDoc {
        name: "height",
        value: "number",
        default: None,
        doc: "Fixed rectangular inset height.",
    },
    ArgDoc {
        name: "scales",
        value: "\"shared\" | \"local\"",
        default: Some("\"shared\""),
        doc: "Child scale training policy across inset instances.",
    },
    ArgDoc {
        name: "guides",
        value: "boolean",
        default: Some("false"),
        doc: "Whether to draw child position guides inside each inset.",
    },
    ArgDoc {
        name: "clip",
        value: "\"rect\" | \"circle\" | false",
        default: Some("\"rect\""),
        doc: "Clip shape for child marks.",
    },
    ArgDoc {
        name: "anchor",
        value: "\"position\" | \"centroid\"",
        default: Some("\"position\""),
        doc: "Parent-row anchor used for inset placement.",
    },
    ArgDoc {
        name: "dx",
        value: "number",
        default: Some("0"),
        doc: "Horizontal pixel offset after anchor resolution.",
    },
    ArgDoc {
        name: "dy",
        value: "number",
        default: Some("0"),
        doc: "Vertical pixel offset after anchor resolution.",
    },
];

/// Human-facing declaration documentation shared by hover providers.
pub fn declaration_doc(name: &str) -> Option<CallDoc> {
    match name {
        "Chart" => Some(CallDoc {
            name: "Chart",
            kind: "Declaration",
            description: "Root chart block with a primary data source and chart-level settings.",
            args: CHART_DOC_ARGS,
            example: "Chart(data: \"data.csv\") {\n    Space(x * y) { Point() }\n}",
        }),
        "Space" => Some(CallDoc {
            name: "Space",
            kind: "Declaration",
            description: "Coordinate space that binds a frame algebra expression to a table.",
            args: SPACE_DOC_ARGS,
            example: "Space(x * y, data: binned) {\n    Rect(xmin: x_min, xmax: x_max, ymin: y_min, ymax: y_max)\n}",
        }),
        "Inset" => Some(CallDoc {
            name: "Inset",
            kind: "Declaration",
            description: "Mark-anchored child plot with explicit row matching and local viewport.",
            args: INSET_DOC_ARGS,
            example: "Inset(data: child, match: [id => id], size: 32) {\n    Space(value) { Bar(fill: group, layout: \"fill\") }\n}",
        }),
        "Theme" => Some(CallDoc {
            name: "Theme",
            kind: "Declaration",
            description: "Selects a named theme and optional source-level overrides.",
            args: THEME_DOC_ARGS,
            example: "Theme(name: \"minimal\", grid: false)",
        }),
        "Scale" => Some(CallDoc {
            name: "Scale",
            kind: "Declaration",
            description: "Overrides an axis or aesthetic scale.",
            args: SCALE_DOC_ARGS,
            example: "Scale(axis: y, type: \"sqrt\")",
        }),
        "Guide" => Some(CallDoc {
            name: "Guide",
            kind: "Declaration",
            description: "Overrides axis labels, tick formatting, grid lines, and legends.",
            args: GUIDE_DOC_ARGS,
            example: "Guide(axis: x, label: \"Flipper length\")",
        }),
        "Layout" => Some(CallDoc {
            name: "Layout",
            kind: "Declaration",
            description: "Configures chart-level layout behavior such as facet wrapping.",
            args: LAYOUT_DOC_ARGS,
            example: "Layout(facetColumns: 2)",
        }),
        "Table" => Some(CallDoc {
            name: "Table",
            kind: "Declaration",
            description: "Declares a chart-scoped named table loaded from its own source.",
            args: TABLE_DOC_ARGS,
            example: "Table cities = \"cities.csv\"",
        }),
        _ => None,
    }
}

/// Concise examples for geometry call hover.
pub fn geometry_example(name: &str) -> &'static str {
    match name {
        "Point" => "Point(fill: species, size: mass)",
        "Line" => "Line(stroke: series, group: series)",
        "Path" => "Path(stroke: series)",
        "Bar" => "Bar(fill: group, layout: \"stack\")",
        "Rect" => "Rect(xmin: x_min, xmax: x_max, ymin: y_min, ymax: y_max)",
        "Histogram" => "Histogram(bins: 30, fill: \"#4f6bed\")",
        "FreqPoly" => "FreqPoly(bins: 30, stroke: group)",
        "Bin2D" => "Bin2D(bins: 20)",
        "HexBin" => "HexBin(bins: 20)",
        "Smooth" => "Smooth(method: \"lm\", se: true)",
        "Boxplot" => "Boxplot(fill: group)",
        "Violin" => "Violin(fill: group, quantiles: [0.25, 0.5, 0.75])",
        "Density" => "Density(bandwidth: 0.8)",
        "ErrorBar" => "ErrorBar(ymin: low, ymax: high)",
        "LineRange" => "LineRange(ymin: low, ymax: high)",
        "PointRange" => "PointRange(ymin: low, ymax: high)",
        "CrossBar" => "CrossBar(ymin: low, ymax: high)",
        "Ribbon" => "Ribbon(ymin: low, ymax: high)",
        "Tile" => "Tile(fill: value)",
        "HLine" => "HLine(y: 0, label: \"baseline\")",
        "VLine" => "VLine(x: 0)",
        "Rug" => "Rug(sides: \"b\")",
        "Area" => "Area(fill: \"#8fb3ff\")",
        "Text" => "Text(label: name, anchor: \"middle\")",
        "Image" => "Image(src: logo, size: 22)",
        "Segment" => "Segment(x: x0, y: y0, xend: x1, yend: y1)",
        "Geo" => "Geo(fill: region)",
        "Graticule" => "Graticule(step: 10)",
        _ => "",
    }
}

/// Concise examples for stat call hover.
pub fn stat_example(name: &str) -> &'static str {
    match name {
        "Bin" => "Derive bins = Bin(value, bins: 30)",
        "Smooth" => "Derive trend = Smooth(x, y, method: \"lm\")",
        "Bin2D" => "Derive binned = Bin2D(x, y, bins: 20)",
        "HexBin" => "Derive hex = HexBin(x, y, bins: 20)",
        "ContourLines" => "Derive lines = ContourLines(x, y, z: value, levels: 10)",
        "ContourBands" => "Derive bands = ContourBands(x, y, z: value, levels: 10)",
        "Density2D" => "Derive density = Density2D(x, y, bandwidth: 1)",
        "Density2DContours" => "Derive lines = Density2DContours(x, y, levels: 8)",
        "Density2DBands" => "Derive bands = Density2DBands(x, y, levels: 8)",
        "Distinct" => "Derive unique = Distinct(species, island)",
        "Ecdf" => "Derive ecdf = Ecdf(latency_ms)",
        "Qq" => "Derive qq = Qq(residual, distribution: \"normal\")",
        "Summary" => "Derive means = Summary(outcome, by: [group], reducer: \"mean_se\")",
        "SummaryBin" => "Derive bins = SummaryBin(x, value, bins: 12)",
        "Cut" => {
            "Derive classes = Cut(score, breaks: [0, 50, 80], labels: [\"low\", \"mid\", \"high\"])"
        }
        "Summary2D" => "Derive cells = Summary2D(x, y, z: value, reducer: \"mean\")",
        "SummaryHex" => "Derive hex = SummaryHex(x, y, z: value, reducer: \"mean\")",
        "StepVertices" => "Derive steps = StepVertices(x, y, direction: \"hv\")",
        "JitterPoints" => "Derive jittered = JitterPoints(x, y, width: 0.2, height: 0)",
        "VectorEndpoints" => "Derive vectors = VectorEndpoints(x, y, angle, length)",
        "CurveSample" => "Derive curves = CurveSample(x, y, xend, yend, points: 24)",
        "IntervalSegments" => "Derive intervals = IntervalSegments(x, low, high)",
        "IntervalRects" => "Derive bands = IntervalRects(x, low, high)",
        "IntervalMiddles" => "Derive mids = IntervalMiddles(x, mid)",
        "Centroid" => "Derive points = Centroid(geom)",
        "Simplify" => "Derive simple = Simplify(geom, tolerance: 0.01)",
        "SpatialJoin" => "Derive joined = SpatialJoin(point_geom, table: regions)",
        _ => "",
    }
}

/// The ordered argument names for a declaration keyword.
pub fn declaration_arg_names(decl: &str) -> &'static [&'static str] {
    match decl {
        "Algraf" => &["version", "features"],
        "Layout" => &[
            "facetColumns",
            "facetRows",
            "facetCols",
            "facetScales",
            "facetLabel",
            "facetLabels",
            "panelSpacing",
        ],
        "Parse" => &[
            "table", "column", "as", "format", "formats", "unit", "timezone", "onError", "anchor",
        ],
        "Guide" => &[
            "axis",
            "label",
            "timeFormat",
            "tickLabelAngle",
            "tickLabelRows",
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
            "mode",
            "breaks",
            "expand",
            "expansion",
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
        "Inset" => &[
            "data",
            "match",
            "size",
            "width",
            "height",
            "minSize",
            "maxSize",
            "scales",
            "guides",
            "clip",
            "padding",
            "placement",
            "dx",
            "dy",
            "anchor",
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
            "nudge",
            "nudgeData",
        ],
        "Stop" => &["value", "color"],
        "Bin" => &["bins", "binWidth", "boundary", "closed", "interval"],
        "ContourLines" => &["z", "levels"],
        "ContourBands" => &["z", "levels"],
        "Density2D" => &["bandwidth", "grid"],
        "Density2DContours" => &["bandwidth", "grid", "levels"],
        "Density2DBands" => &["bandwidth", "grid", "levels"],
        "Distinct" => &[],
        "Ecdf" => &[],
        "Qq" => &["distribution", "reference"],
        "Summary" => &["by", "reducer"],
        "SummaryBin" => &["by", "bins", "binWidth", "boundary", "closed", "reducer"],
        "Cut" => &["breaks", "labels", "output"],
        "Summary2D" => &["z", "bins", "reducer"],
        "SummaryHex" => &["z", "bins", "reducer"],
        "Smooth" => &["method", "span", "se"],
        "StepVertices" => &["direction"],
        "JitterPoints" => &["width", "height"],
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
        "ContourLines" => "Derives isoline vertices from regular x/y/z gridded data.",
        "ContourBands" => "Derives filled contour-band geometries from regular x/y/z gridded data.",
        "Density2D" => "Derives a deterministic two-dimensional Gaussian KDE grid.",
        "Density2DContours" => "Derives isoline vertices from a two-dimensional KDE.",
        "Density2DBands" => "Derives filled contour-band geometries from a two-dimensional KDE.",
        "Distinct" => "Derives first-retained distinct source rows by the input columns.",
        "Ecdf" => "Derives empirical cumulative distribution vertices with x and y columns.",
        "Qq" => "Derives normal QQ sample and theoretical quantile rows.",
        "Summary" => "Aggregates a value column, optionally by grouping columns.",
        "SummaryBin" => "Aggregates values in deterministic bins over a continuous x column.",
        "Cut" => "Derives a binned class column from numeric break values.",
        "Summary2D" => "Aggregates a z column into rectangular x/y bins.",
        "SummaryHex" => "Aggregates a z column into hexagonal x/y bins.",
        "StepVertices" => "Expands source x/y rows into orthogonal Path vertices.",
        "JitterPoints" => "Derives deterministic x/y point coordinates with data-space jitter.",
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
const SRC: &[Accept] = &[Accept::Column, Accept::Str];
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
const POSITION_ADJUST: &[Accept] = &[Accept::NumberArray];

const POINT: &[PropSpec] = &[
    opt(PropertyKey::Fill, FILL),
    opt(PropertyKey::Stroke, STROKE),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Size, SIZE),
    opt(PropertyKey::Shape, SHAPE),
    opt(PropertyKey::Jitter, POSITION_ADJUST),
    opt(PropertyKey::Nudge, POSITION_ADJUST),
    opt(PropertyKey::NudgeData, POSITION_ADJUST),
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
    opt(PropertyKey::Nudge, POSITION_ADJUST),
    opt(PropertyKey::NudgeData, POSITION_ADJUST),
    opt(PropertyKey::Declutter, &[Accept::Bool]),
    // Format a temporal `label:` column using the §19.4 named/custom model.
    opt(PropertyKey::TimeFormat, &[Accept::Str]),
];

const IMAGE: &[PropSpec] = &[
    req(PropertyKey::Src, SRC),
    opt(PropertyKey::Alpha, ALPHA),
    opt(PropertyKey::Size, SIZE),
    opt(PropertyKey::Jitter, POSITION_ADJUST),
    opt(PropertyKey::Nudge, POSITION_ADJUST),
    opt(PropertyKey::NudgeData, POSITION_ADJUST),
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
    geo(GeometryKind::Image, IMAGE),
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
/// the per-datum filled marks: `Point`, `Image`, `Bar`, `Rect`, and `Tile`.
pub fn supports_interaction(kind: GeometryKind) -> bool {
    matches!(
        kind,
        GeometryKind::Point
            | GeometryKind::Image
            | GeometryKind::Bar
            | GeometryKind::Rect
            | GeometryKind::Tile
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
        "Image" => "Draws one local image per row in the inherited space.",
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
        "src" => "Local image source path or string column mapping.",
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
        "jitter" => "Deterministic point jitter as [x, y] in data units for continuous axes or band fractions for categorical axes.",
        "nudge" => "Pixel-space position offset as [dx, dy].",
        "nudgeData" => "Data-space position offset as [dx, dy] before scale mapping.",
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
