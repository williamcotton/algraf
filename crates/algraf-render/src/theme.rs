//! Themes and color palettes (spec §16.8–16.9, §20).

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
    pub text_color: String,
    pub title_size: f64,
    pub point_size: f64,
    pub line_width: f64,
    /// Whether grid lines are drawn.
    pub grid: bool,
    /// Whether axes are drawn.
    pub axes: bool,
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
            _ => Theme::minimal(),
        }
    }

    fn base() -> Theme {
        Theme {
            name: "minimal",
            font_family: "system-ui, sans-serif".to_string(),
            font_size: 12.0,
            background: "#ffffff".to_string(),
            plot_background: "#ffffff".to_string(),
            axis_color: "#333333".to_string(),
            grid_major_color: "#e6e6e6".to_string(),
            text_color: "#222222".to_string(),
            title_size: 18.0,
            point_size: 3.0,
            line_width: 1.5,
            grid: true,
            axes: true,
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
    let t = t.clamp(0.0, 1.0);
    let stops = CONTINUOUS_GRADIENT;
    let segments = stops.len() - 1;
    let scaled = t * segments as f64;
    let i = (scaled.floor() as usize).min(segments - 1);
    let local = scaled - i as f64;
    let (r1, g1, b1) = parse_hex(stops[i]);
    let (r2, g2, b2) = parse_hex(stops[i + 1]);
    let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * local).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        lerp(r1, r2),
        lerp(g1, g2),
        lerp(b1, b2)
    )
}

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}
