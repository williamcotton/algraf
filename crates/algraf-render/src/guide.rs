//! Axis, grid, and legend rendering (spec §19).

use crate::aes::{Legend, LegendKind};
use crate::layout::Rect;
use crate::space::ScaledSpace;
use crate::svg::{escape_attr, escape_text, num, SvgWriter};
use crate::theme::Theme;

/// Draw grid lines behind the data marks (spec §17.6). Only continuous and
/// temporal axes get grid lines; categorical axes do not.
pub fn render_grid(w: &mut SvgWriter, space: &ScaledSpace, plot: Rect, theme: &Theme) {
    if !theme.grid {
        return;
    }
    w.open_group("class=\"algraf-grid\"");
    if !space.x.is_band() {
        for (x, _) in space.x.ticks() {
            w.line(&grid_line(
                x,
                plot.y,
                x,
                plot.bottom(),
                &theme.grid_major_color,
            ));
        }
    }
    if let Some(y) = &space.y {
        if !y.is_band() {
            for (yp, _) in y.ticks() {
                w.line(&grid_line(
                    plot.x,
                    yp,
                    plot.right(),
                    yp,
                    &theme.grid_major_color,
                ));
            }
        }
    }
    w.close_group();
}

fn grid_line(x1: f64, y1: f64, x2: f64, y2: f64, color: &str) -> String {
    format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"1\" />",
        num(x1),
        num(y1),
        num(x2),
        num(y2),
        escape_attr(color),
    )
}

/// Draw x and y axes with ticks, labels, and titles (spec §19.1–19.4).
pub fn render_axes(w: &mut SvgWriter, space: &ScaledSpace, plot: Rect, theme: &Theme) {
    w.open_group("class=\"algraf-axes\"");

    // X axis along the bottom.
    w.line(&grid_line(
        plot.x,
        plot.bottom(),
        plot.right(),
        plot.bottom(),
        &theme.axis_color,
    ));
    for (x, label) in space.x.ticks() {
        w.line(&grid_line(
            x,
            plot.bottom(),
            x,
            plot.bottom() + 5.0,
            &theme.axis_color,
        ));
        w.line(&text(x, plot.bottom() + 18.0, "middle", &label, theme));
    }
    w.line(&text(
        plot.x + plot.width / 2.0,
        plot.bottom() + 38.0,
        "middle",
        &space.x.label(),
        theme,
    ));

    // Y axis along the left.
    if let Some(y) = &space.y {
        w.line(&grid_line(
            plot.x,
            plot.y,
            plot.x,
            plot.bottom(),
            &theme.axis_color,
        ));
        for (yp, label) in y.ticks() {
            w.line(&grid_line(plot.x - 5.0, yp, plot.x, yp, &theme.axis_color));
            w.line(&text(plot.x - 8.0, yp + 4.0, "end", &label, theme));
        }
        let cy = plot.y + plot.height / 2.0;
        w.line(&format!(
            "<text x=\"16\" y=\"{}\" text-anchor=\"middle\" transform=\"rotate(-90 16 {})\" \
             font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(cy),
            num(cy),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(&y.label()),
        ));
    }

    w.close_group();
}

fn text(x: f64, y: f64, anchor: &str, content: &str, theme: &Theme) -> String {
    format!(
        "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
        num(x),
        num(y),
        anchor,
        escape_attr(&theme.font_family),
        num(theme.font_size),
        escape_attr(&theme.text_color),
        escape_text(content),
    )
}

/// Draw legends for mapped aesthetics (spec §19.5).
pub fn render_legends(w: &mut SvgWriter, legends: &[Legend], area: Rect, theme: &Theme) {
    if legends.is_empty() {
        return;
    }
    w.open_group("class=\"algraf-legends\"");
    let mut y = area.y + 4.0;
    for legend in legends {
        w.line(&text(area.x, y, "start", &legend.title, theme));
        y += 18.0;
        match legend.kind {
            LegendKind::Discrete => {
                for (label, color) in &legend.entries {
                    w.line(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"12\" fill=\"{}\" />",
                        num(area.x),
                        num(y - 10.0),
                        escape_attr(color),
                    ));
                    w.line(&text(area.x + 18.0, y, "start", label, theme));
                    y += 18.0;
                }
            }
            LegendKind::Continuous => {
                y = render_continuous_legend(w, legend, area.x, y, theme);
            }
        }
        y += 8.0;
    }
    w.close_group();
}

fn render_continuous_legend(
    w: &mut SvgWriter,
    legend: &Legend,
    x: f64,
    y: f64,
    theme: &Theme,
) -> f64 {
    if legend.entries.is_empty() {
        return y;
    }
    let step = 16.0;
    for (index, (label, color)) in legend.entries.iter().rev().enumerate() {
        let y0 = y + index as f64 * step;
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"{}\" fill=\"{}\" />",
            num(x),
            num(y0 - 10.0),
            num(step),
            escape_attr(color),
        ));
        w.line(&text(x + 18.0, y0, "start", label, theme));
    }
    y + legend.entries.len() as f64 * step
}
