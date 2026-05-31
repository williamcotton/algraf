//! Themes and color palettes (spec §16.8–16.9, §20).

use algraf_semantics::{
    LegendPositionIr, ThemeIr, ThemeLineIr, ThemeOverrides, ThemeRectIr, ThemeTextIr,
};

/// Concrete text style for a theme element.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_family: String,
    pub size: f64,
    pub fill: String,
}

/// Concrete line style for guides.
#[derive(Debug, Clone, PartialEq)]
pub struct LineStyle {
    pub stroke: String,
    pub stroke_width: f64,
}

/// Concrete rectangle style for backgrounds.
#[derive(Debug, Clone, PartialEq)]
pub struct RectStyle {
    pub fill: String,
    pub stroke: Option<String>,
    pub stroke_width: f64,
}

/// A visual theme (spec §20.1). Colors are stored as SVG color strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub font_family: String,
    pub font_size: f64,
    pub background: String,
    pub plot_background: String,
    pub axis_color: String,
    pub grid_major_color: String,
    /// Stroke width of major grid lines in pixels (spec §20.1).
    pub grid_major_width: f64,
    pub text_color: String,
    pub title_size: f64,
    pub point_size: f64,
    pub line_width: f64,
    /// Whether grid lines are drawn.
    pub grid: bool,
    /// Whether axes are drawn.
    pub axes: bool,
    pub plot_title: TextStyle,
    pub plot_subtitle: TextStyle,
    pub plot_caption: TextStyle,
    pub axis_title: TextStyle,
    pub axis_text: TextStyle,
    pub strip_text: TextStyle,
    pub legend_title: TextStyle,
    pub legend_text: TextStyle,
    pub panel_background: RectStyle,
    pub grid_major: LineStyle,
    pub grid_minor: LineStyle,
    pub legend_position: LegendPositionIr,
    pub legend_spacing: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::minimal()
    }
}

impl Theme {
    /// Resolve a theme by name, falling back to `minimal` (spec §20.1).
    pub fn by_name(name: &str) -> Theme {
        match name {
            "classic" => Theme::classic(),
            "light" => Theme::light(),
            "dark" => Theme::dark(),
            "void" => Theme::void(),
            "gray" => Theme::gray(),
            "bw" => Theme::bw(),
            "linedraw" => Theme::linedraw(),
            _ => Theme::minimal(),
        }
    }

    fn base() -> Theme {
        let font_family = "system-ui, sans-serif";
        let text_color = "#222222";
        let font_size = 12.0;
        let title_size = 18.0;
        Theme {
            name: "minimal",
            font_family: font_family.to_string(),
            font_size,
            background: "#ffffff".to_string(),
            plot_background: "#ffffff".to_string(),
            axis_color: "#333333".to_string(),
            grid_major_color: "#e6e6e6".to_string(),
            grid_major_width: 1.0,
            text_color: text_color.to_string(),
            title_size,
            point_size: 3.0,
            line_width: 1.5,
            grid: true,
            axes: true,
            plot_title: text_style(font_family, title_size, text_color),
            plot_subtitle: text_style(font_family, font_size, text_color),
            plot_caption: text_style(font_family, font_size, text_color),
            axis_title: text_style(font_family, font_size, text_color),
            axis_text: text_style(font_family, font_size, text_color),
            strip_text: text_style(font_family, font_size, text_color),
            legend_title: text_style(font_family, font_size, text_color),
            legend_text: text_style(font_family, font_size, text_color),
            panel_background: rect_style("#ffffff"),
            grid_major: line_style("#e6e6e6", 1.0),
            grid_minor: line_style("#f2f2f2", 0.6),
            legend_position: LegendPositionIr::Right,
            legend_spacing: 20.0,
        }
    }

    pub fn minimal() -> Theme {
        Theme::base()
    }

    pub fn classic() -> Theme {
        Theme {
            name: "classic",
            grid: false,
            ..Theme::base()
        }
    }

    pub fn light() -> Theme {
        Theme {
            name: "light",
            background: "#f8fafc".to_string(),
            plot_background: "#ffffff".to_string(),
            panel_background: rect_style("#ffffff"),
            ..Theme::base()
        }
    }

    pub fn dark() -> Theme {
        Theme {
            name: "dark",
            background: "#1e1e1e".to_string(),
            plot_background: "#1e1e1e".to_string(),
            axis_color: "#cccccc".to_string(),
            grid_major_color: "#3a3a3a".to_string(),
            text_color: "#eeeeee".to_string(),
            plot_title: text_style("system-ui, sans-serif", 18.0, "#f5f5f5"),
            plot_subtitle: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            plot_caption: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            axis_title: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            axis_text: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            strip_text: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            legend_title: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            legend_text: text_style("system-ui, sans-serif", 12.0, "#eeeeee"),
            panel_background: rect_style("#1e1e1e"),
            grid_major: line_style("#3a3a3a", 1.0),
            grid_minor: line_style("#2a2a2a", 0.6),
            ..Theme::base()
        }
    }

    pub fn void() -> Theme {
        Theme {
            name: "void",
            grid: false,
            axes: false,
            ..Theme::base()
        }
    }

    pub fn gray() -> Theme {
        let mut theme = Theme {
            name: "gray",
            background: "#ffffff".to_string(),
            plot_background: "#ebebeb".to_string(),
            axis_color: "#4d4d4d".to_string(),
            grid_major_color: "#ffffff".to_string(),
            grid_major_width: 1.0,
            text_color: "#1f1f1f".to_string(),
            panel_background: rect_style("#ebebeb"),
            grid_major: line_style("#ffffff", 1.0),
            grid_minor: line_style("#f7f7f7", 0.6),
            ..Theme::base()
        };
        theme.set_all_text_fill("#1f1f1f");
        theme
    }

    pub fn bw() -> Theme {
        let mut theme = Theme {
            name: "bw",
            background: "#ffffff".to_string(),
            plot_background: "#ffffff".to_string(),
            axis_color: "#111111".to_string(),
            grid_major_color: "#d9d9d9".to_string(),
            text_color: "#111111".to_string(),
            panel_background: RectStyle {
                fill: "#ffffff".to_string(),
                stroke: Some("#111111".to_string()),
                stroke_width: 1.0,
            },
            grid_major: line_style("#d9d9d9", 0.8),
            grid_minor: line_style("#eeeeee", 0.5),
            ..Theme::base()
        };
        theme.set_all_text_fill("#111111");
        theme
    }

    pub fn linedraw() -> Theme {
        let mut theme = Theme {
            name: "linedraw",
            background: "#ffffff".to_string(),
            plot_background: "#ffffff".to_string(),
            axis_color: "#000000".to_string(),
            grid_major_color: "#bdbdbd".to_string(),
            grid_major_width: 0.6,
            text_color: "#000000".to_string(),
            panel_background: RectStyle {
                fill: "#ffffff".to_string(),
                stroke: Some("#000000".to_string()),
                stroke_width: 0.6,
            },
            grid_major: line_style("#bdbdbd", 0.6),
            grid_minor: line_style("#e5e5e5", 0.4),
            line_width: 0.8,
            ..Theme::base()
        };
        theme.set_all_text_fill("#000000");
        theme
    }

    /// Resolve a [`ThemeIr`]: select the named base (defaulting to `minimal`)
    /// and layer its overrides on top (spec §20.1, §20.8).
    pub fn from_ir(ir: &ThemeIr) -> Theme {
        let mut theme = match &ir.base {
            Some(name) => Theme::by_name(name),
            None => Theme::default(),
        };
        theme.apply_overrides(&ir.overrides);
        theme
    }

    /// Apply per-field overrides on top of this theme (spec §20.8).
    pub fn apply_overrides(&mut self, ov: &ThemeOverrides) {
        if let Some(v) = &ov.font_family {
            self.font_family = v.clone();
            self.set_all_text_family(v);
        }
        if let Some(v) = ov.font_size {
            self.font_size = v;
            self.set_base_text_size(v);
        }
        if let Some(v) = &ov.background {
            self.background = v.clone();
        }
        if let Some(v) = &ov.plot_background {
            self.plot_background = v.clone();
            self.panel_background.fill = v.clone();
        }
        if let Some(v) = &ov.axis_color {
            self.axis_color = v.clone();
        }
        if let Some(v) = &ov.grid_major_color {
            self.grid_major_color = v.clone();
            self.grid_major.stroke = v.clone();
        }
        if let Some(v) = ov.grid_major_width {
            self.grid_major_width = v;
            self.grid_major.stroke_width = v;
        }
        if let Some(v) = &ov.text_color {
            self.text_color = v.clone();
            self.set_all_text_fill(v);
        }
        if let Some(v) = ov.title_size {
            self.title_size = v;
            self.plot_title.size = v;
        }
        if let Some(v) = ov.point_size {
            self.point_size = v;
        }
        if let Some(v) = ov.line_width {
            self.line_width = v;
        }
        if let Some(v) = ov.grid {
            self.grid = v;
        }
        if let Some(v) = ov.axes {
            self.axes = v;
        }
        if let Some(v) = &ov.plot_title {
            apply_text_override(&mut self.plot_title, v);
            self.title_size = self.plot_title.size;
        }
        if let Some(v) = &ov.plot_subtitle {
            apply_text_override(&mut self.plot_subtitle, v);
        }
        if let Some(v) = &ov.plot_caption {
            apply_text_override(&mut self.plot_caption, v);
        }
        if let Some(v) = &ov.axis_title {
            apply_text_override(&mut self.axis_title, v);
        }
        if let Some(v) = &ov.axis_text {
            apply_text_override(&mut self.axis_text, v);
            self.font_size = self.axis_text.size;
        }
        if let Some(v) = &ov.strip_text {
            apply_text_override(&mut self.strip_text, v);
        }
        if let Some(v) = &ov.legend_title {
            apply_text_override(&mut self.legend_title, v);
        }
        if let Some(v) = &ov.legend_text {
            apply_text_override(&mut self.legend_text, v);
        }
        if let Some(v) = &ov.panel_background {
            apply_rect_override(&mut self.panel_background, v);
            self.plot_background = self.panel_background.fill.clone();
        }
        if let Some(v) = &ov.grid_major {
            apply_line_override(&mut self.grid_major, v);
            self.grid_major_color = self.grid_major.stroke.clone();
            self.grid_major_width = self.grid_major.stroke_width;
        }
        if let Some(v) = &ov.grid_minor {
            apply_line_override(&mut self.grid_minor, v);
        }
        if let Some(v) = ov.legend_position {
            self.legend_position = v;
        }
        if let Some(v) = ov.legend_spacing {
            self.legend_spacing = v.max(0.0);
        }
    }

    fn set_all_text_family(&mut self, family: &str) {
        for style in [
            &mut self.plot_title,
            &mut self.plot_subtitle,
            &mut self.plot_caption,
            &mut self.axis_title,
            &mut self.axis_text,
            &mut self.strip_text,
            &mut self.legend_title,
            &mut self.legend_text,
        ] {
            style.font_family = family.to_string();
        }
    }

    fn set_base_text_size(&mut self, size: f64) {
        for style in [
            &mut self.plot_subtitle,
            &mut self.plot_caption,
            &mut self.axis_title,
            &mut self.axis_text,
            &mut self.strip_text,
            &mut self.legend_title,
            &mut self.legend_text,
        ] {
            style.size = size;
        }
    }

    fn set_all_text_fill(&mut self, fill: &str) {
        for style in [
            &mut self.plot_title,
            &mut self.plot_subtitle,
            &mut self.plot_caption,
            &mut self.axis_title,
            &mut self.axis_text,
            &mut self.strip_text,
            &mut self.legend_title,
            &mut self.legend_text,
        ] {
            style.fill = fill.to_string();
        }
    }
}

fn text_style(font_family: &str, size: f64, fill: &str) -> TextStyle {
    TextStyle {
        font_family: font_family.to_string(),
        size,
        fill: fill.to_string(),
    }
}

fn line_style(stroke: &str, stroke_width: f64) -> LineStyle {
    LineStyle {
        stroke: stroke.to_string(),
        stroke_width,
    }
}

fn rect_style(fill: &str) -> RectStyle {
    RectStyle {
        fill: fill.to_string(),
        stroke: None,
        stroke_width: 0.0,
    }
}

fn apply_text_override(style: &mut TextStyle, override_: &ThemeTextIr) {
    if let Some(v) = &override_.font_family {
        style.font_family = v.clone();
    }
    if let Some(v) = override_.size {
        style.size = v;
    }
    if let Some(v) = &override_.fill {
        style.fill = v.clone();
    }
}

fn apply_line_override(style: &mut LineStyle, override_: &ThemeLineIr) {
    if let Some(v) = &override_.stroke {
        style.stroke = v.clone();
    }
    if let Some(v) = override_.stroke_width {
        style.stroke_width = v;
    }
}

fn apply_rect_override(style: &mut RectStyle, override_: &ThemeRectIr) {
    if let Some(v) = &override_.fill {
        style.fill = v.clone();
    }
    if let Some(v) = &override_.stroke {
        style.stroke = Some(v.clone());
    }
    if let Some(v) = override_.stroke_width {
        style.stroke_width = v;
    }
}

/// The default categorical palette (spec §16.9), colorblind-aware.
pub const CATEGORICAL_PALETTE: &[&str] = &[
    "#4E79A7", "#F28E2B", "#E15759", "#76B7B2", "#59A14F", "#EDC948", "#B07AA1", "#FF9DA7",
    "#9C755F", "#BAB0AC",
];

/// A higher-contrast categorical palette for `Scale(..., palette: "accent")`.
pub const ACCENT_PALETTE: &[&str] = &[
    "#006BA4", "#FF800E", "#ABABAB", "#595959", "#5F9ED1", "#C85200", "#898989", "#A2C8EC",
    "#FFBC79", "#CFCFCF",
];

/// The default continuous gradient stops (spec §16.8), perceptually ordered.
pub const CONTINUOUS_GRADIENT: &[&str] = &["#440154", "#31688E", "#35B779", "#FDE725"];

pub fn palette_colors(name: Option<&str>) -> &'static [&'static str] {
    match name {
        Some("accent") => ACCENT_PALETTE,
        _ => CATEGORICAL_PALETTE,
    }
}

/// Pick a categorical color from a named palette by stable index.
pub fn categorical_color_from(name: Option<&str>, index: usize) -> &'static str {
    let palette = palette_colors(name);
    palette[index % palette.len()]
}

/// Interpolate the continuous gradient at `t` in `[0, 1]`, returning a hex color.
pub fn gradient_color(t: f64) -> String {
    gradient_color_from(CONTINUOUS_GRADIENT, t)
}

pub fn gradient_color_from(stops: &[&str], t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    if stops.is_empty() {
        return "#000000".to_string();
    }
    if stops.len() == 1 {
        let (r, g, b) = parse_color(stops[0]);
        return format!("#{r:02x}{g:02x}{b:02x}");
    }
    let segments = stops.len() - 1;
    let scaled = t * segments as f64;
    let i = (scaled.floor() as usize).min(segments - 1);
    let local = scaled - i as f64;
    let (r1, g1, b1) = parse_color(stops[i]);
    let (r2, g2, b2) = parse_color(stops[i + 1]);
    let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * local).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        lerp(r1, r2),
        lerp(g1, g2),
        lerp(b1, b2)
    )
}

fn parse_color(color: &str) -> (u8, u8, u8) {
    match color.to_ascii_lowercase().as_str() {
        "black" => return (0, 0, 0),
        "white" => return (255, 255, 255),
        "red" => return (255, 0, 0),
        "green" => return (0, 128, 0),
        "blue" => return (0, 0, 255),
        "steelblue" => return (70, 130, 180),
        "orange" => return (255, 165, 0),
        "purple" => return (128, 0, 128),
        _ => {}
    }
    let h = color.trim_start_matches('#');
    if h.len() == 3 {
        let expand = |i: usize| {
            u8::from_str_radix(&h[i..=i], 16)
                .map(|v| v * 17)
                .unwrap_or(0)
        };
        return (expand(0), expand(1), expand(2));
    }
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
        return (r, g, b);
    }
    (0, 0, 0)
}
