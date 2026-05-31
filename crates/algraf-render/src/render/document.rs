use algraf_core::Diagnostic;
use algraf_semantics::ChartIr;

use crate::guide;
use crate::layout::Layout;
use crate::sink::{MarkSink, SvgSink};
use crate::svg::{escape_attr, escape_text, num, SvgAttr, SvgWriter};
use crate::theme::Theme;

use super::backend::RenderScene;
use super::panels::{panel_slots, Panel, PanelSlot};

/// The fixed, audited interactive runtime embedded when `--interactive` is set
/// (spec §29.3). It is identical for every chart and is the *only* path by which
/// a `<script>` enters Algraf output. It reads the inert per-mark metadata the
/// SVG backend already emits — `<title>` tooltips and `data-algraf-highlight`
/// groups — plus the emitted plot rectangles and tick labels for a pointer
/// crosshair/value readout. Chart source can never supply or extend it; it
/// performs no network access and is deterministic given the same SVG.
const INTERACTIVE_RUNTIME: &str = r##"(function() {
  "use strict";

  var NS = "http://www.w3.org/2000/svg";
  var script = document.currentScript;
  var root = script && script.closest ? script.closest("svg") : null;
  if (!root) {
    var svgs = document.getElementsByTagName("svg");
    root = svgs[svgs.length - 1];
  }
  if (!root) return;

  var supportsPointer = typeof PointerEvent !== "undefined";
  var enterEvent = supportsPointer ? "pointerenter" : "mouseenter";
  var moveEvent = supportsPointer ? "pointermove" : "mousemove";
  var leaveEvent = supportsPointer ? "pointerleave" : "mouseleave";

  function svgEl(name) {
    return document.createElementNS(NS, name);
  }

  function attrNumber(el, name) {
    var value = parseFloat(el.getAttribute(name) || "");
    return isFinite(value) ? value : null;
  }

  function clamp(value, min, max) {
    if (max < min) return min;
    return Math.max(min, Math.min(max, value));
  }

  function clear(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  function setLines(text, lines) {
    clear(text);
    for (var i = 0; i < lines.length; i++) {
      var tspan = svgEl("tspan");
      tspan.setAttribute("x", "8");
      tspan.setAttribute("dy", i === 0 ? "16" : "14");
      tspan.textContent = lines[i];
      text.appendChild(tspan);
    }
  }

  function fallbackTextSize(lines) {
    var width = 0;
    for (var i = 0; i < lines.length; i++) {
      width = Math.max(width, lines[i].length * 7);
    }
    return { width: width, height: Math.max(14, lines.length * 14) };
  }

  function sizeBox(bg, text, lines) {
    var size = fallbackTextSize(lines);
    try {
      var bbox = text.getBBox();
      if (bbox.width > 0 || bbox.height > 0) {
        size = { width: bbox.width, height: bbox.height };
      }
    } catch (_) {}
    var width = Math.max(36, size.width + 16);
    var height = Math.max(24, size.height + 10);
    bg.setAttribute("width", width);
    bg.setAttribute("height", height);
    return { width: width, height: height };
  }

  function svgBounds() {
    var viewBox = root.viewBox && root.viewBox.baseVal;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      return { x: viewBox.x, y: viewBox.y, width: viewBox.width, height: viewBox.height };
    }
    return {
      x: 0,
      y: 0,
      width: attrNumber(root, "width") || 0,
      height: attrNumber(root, "height") || 0
    };
  }

  function eventPoint(evt) {
    var ctm = root.getScreenCTM && root.getScreenCTM();
    if (!ctm) return null;
    try {
      var pt = root.createSVGPoint();
      pt.x = evt.clientX;
      pt.y = evt.clientY;
      return pt.matrixTransform(ctm.inverse());
    } catch (_) {
      return null;
    }
  }

  function elementPoint(el) {
    try {
      var bbox = el.getBBox();
      return { x: bbox.x + bbox.width, y: bbox.y };
    } catch (_) {
      return { x: 0, y: 0 };
    }
  }

  function placeBox(group, bg, text, lines, anchor, bounds, preferAbove) {
    var box = sizeBox(bg, text, lines);
    var gap = 12;
    var tx = anchor.x + gap;
    var ty = preferAbove ? anchor.y - box.height - gap : anchor.y + gap;
    var minX = bounds.x + 4;
    var minY = bounds.y + 4;
    var maxX = bounds.x + bounds.width - box.width - 4;
    var maxY = bounds.y + bounds.height - box.height - 4;
    if (tx > maxX) tx = anchor.x - box.width - gap;
    if (ty > maxY) ty = anchor.y - box.height - gap;
    if (ty < minY) ty = anchor.y + gap;
    tx = clamp(tx, minX, maxX);
    ty = clamp(ty, minY, maxY);
    group.setAttribute("transform", "translate(" + tx + "," + ty + ")");
    return box;
  }

  function finiteNumber(label) {
    var value = label.replace(/,/g, "").trim();
    if (!/^[+-]?(?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/.test(value)) {
      return null;
    }
    var number = +value;
    return isFinite(number) ? number : null;
  }

  function trimNumber(text) {
    if (text.indexOf("e") !== -1) {
      return text.replace(/\.?0+e/, "e").replace("e+", "e");
    }
    while (text.indexOf(".") !== -1 && text.charAt(text.length - 1) === "0") {
      text = text.slice(0, -1);
    }
    if (text.charAt(text.length - 1) === ".") text = text.slice(0, -1);
    return text === "-0" ? "0" : text;
  }

  function formatNumber(value) {
    if (!isFinite(value)) return "";
    var abs = Math.abs(value);
    if (abs > 0 && (abs >= 100000 || abs < 0.001)) {
      return trimNumber(value.toExponential(3));
    }
    var digits = abs >= 100 ? 1 : abs >= 10 ? 2 : 3;
    return trimNumber(value.toFixed(digits));
  }

  function valueAt(ticks, pos) {
    if (!ticks.length) return null;
    var numeric = [];
    for (var i = 0; i < ticks.length; i++) {
      if (ticks[i].number !== null) numeric.push(ticks[i]);
    }
    if (numeric.length >= 2) {
      var a = numeric[0];
      var b = numeric[numeric.length - 1];
      for (var j = 1; j < numeric.length; j++) {
        var lo = numeric[j - 1];
        var hi = numeric[j];
        if ((pos >= lo.pos && pos <= hi.pos) || (pos >= hi.pos && pos <= lo.pos)) {
          a = lo;
          b = hi;
          break;
        }
      }
      if (Math.abs(b.pos - a.pos) > 1e-9) {
        var t = (pos - a.pos) / (b.pos - a.pos);
        return formatNumber(a.number + t * (b.number - a.number));
      }
    }
    var nearest = ticks[0];
    var distance = Math.abs(pos - nearest.pos);
    for (var k = 1; k < ticks.length; k++) {
      var next = Math.abs(pos - ticks[k].pos);
      if (next < distance) {
        nearest = ticks[k];
        distance = next;
      }
    }
    return nearest.label;
  }

  function axisTicks(area, texts, axis) {
    var ticks = [];
    for (var i = 0; i < texts.length; i++) {
      var text = texts[i];
      var label = (text.textContent || "").trim();
      var x = attrNumber(text, "x");
      var y = attrNumber(text, "y");
      if (!label || x === null || y === null) continue;
      if (axis === "x") {
        if (x < area.x - 1 || x > area.x + area.width + 1) continue;
        if (y <= area.y + area.height || y > area.y + area.height + 32) continue;
        ticks.push({ pos: x, label: label, number: finiteNumber(label) });
      } else {
        if (y < area.y - 8 || y > area.y + area.height + 12) continue;
        if (x >= area.x || x < area.x - 160) continue;
        if ((text.getAttribute("text-anchor") || "") !== "end") continue;
        ticks.push({ pos: y - 4, label: label, number: finiteNumber(label) });
      }
    }
    ticks.sort(function(a, b) {
      return a.pos - b.pos;
    });
    return ticks;
  }

  function readPlotAreas() {
    var rects = root.querySelectorAll("rect.algraf-plot-area");
    var texts = root.querySelectorAll("g.algraf-axes text");
    var areas = [];
    for (var i = 0; i < rects.length; i++) {
      var rect = rects[i];
      var x = attrNumber(rect, "x");
      var y = attrNumber(rect, "y");
      var width = attrNumber(rect, "width");
      var height = attrNumber(rect, "height");
      if (x === null || y === null || width === null || height === null) continue;
      var area = { x: x, y: y, width: width, height: height, el: rect };
      area.xTicks = axisTicks(area, texts, "x");
      area.yTicks = axisTicks(area, texts, "y");
      if (area.xTicks.length || area.yTicks.length) {
        rect.style.cursor = "crosshair";
        areas.push(area);
      }
    }
    return areas;
  }

  function areaAt(point) {
    for (var i = 0; i < plotAreas.length; i++) {
      var area = plotAreas[i];
      if (
        point.x >= area.x &&
        point.x <= area.x + area.width &&
        point.y >= area.y &&
        point.y <= area.y + area.height
      ) {
        return area;
      }
    }
    return null;
  }

  var tooltip = svgEl("g");
  tooltip.setAttribute("class", "algraf-tooltip");
  tooltip.setAttribute("pointer-events", "none");
  tooltip.style.display = "none";
  var tooltipBg = svgEl("rect");
  tooltipBg.setAttribute("fill", "#ffffff");
  tooltipBg.setAttribute("stroke", "#30343b");
  tooltipBg.setAttribute("stroke-width", "1");
  tooltipBg.setAttribute("rx", "3");
  var tooltipText = svgEl("text");
  tooltipText.setAttribute("font-family", "system-ui, sans-serif");
  tooltipText.setAttribute("font-size", "12");
  tooltipText.setAttribute("fill", "#1f2933");
  tooltip.appendChild(tooltipBg);
  tooltip.appendChild(tooltipText);
  root.appendChild(tooltip);

  var crosshair = svgEl("g");
  crosshair.setAttribute("class", "algraf-crosshair");
  crosshair.setAttribute("pointer-events", "none");
  crosshair.style.display = "none";
  var vLine = svgEl("line");
  var hLine = svgEl("line");
  for (var ci = 0, lines = [vLine, hLine]; ci < lines.length; ci++) {
    lines[ci].setAttribute("stroke", "#30343b");
    lines[ci].setAttribute("stroke-width", "1");
    lines[ci].setAttribute("stroke-dasharray", "4 3");
    lines[ci].setAttribute("opacity", "0.72");
    lines[ci].setAttribute("vector-effect", "non-scaling-stroke");
  }
  var crossLabel = svgEl("g");
  var crossBg = svgEl("rect");
  crossBg.setAttribute("fill", "#ffffff");
  crossBg.setAttribute("stroke", "#30343b");
  crossBg.setAttribute("stroke-width", "1");
  crossBg.setAttribute("rx", "3");
  var crossText = svgEl("text");
  crossText.setAttribute("font-family", "system-ui, sans-serif");
  crossText.setAttribute("font-size", "12");
  crossText.setAttribute("fill", "#1f2933");
  crossLabel.appendChild(crossBg);
  crossLabel.appendChild(crossText);
  crosshair.appendChild(vLine);
  crosshair.appendChild(hLine);
  crosshair.appendChild(crossLabel);
  root.appendChild(crosshair);

  var plotAreas = readPlotAreas();

  function hideCrosshair() {
    crosshair.style.display = "none";
  }

  function updateCrosshair(evt) {
    var point = eventPoint(evt);
    if (!point) return;
    var area = areaAt(point);
    if (!area) {
      hideCrosshair();
      return;
    }
    var x = clamp(point.x, area.x, area.x + area.width);
    var y = clamp(point.y, area.y, area.y + area.height);
    var labels = [];
    var xValue = valueAt(area.xTicks, x);
    var yValue = valueAt(area.yTicks, y);
    if (xValue !== null) labels.push("x: " + xValue);
    if (yValue !== null) labels.push("y: " + yValue);
    if (!labels.length) {
      hideCrosshair();
      return;
    }

    vLine.setAttribute("x1", x);
    vLine.setAttribute("x2", x);
    vLine.setAttribute("y1", area.y);
    vLine.setAttribute("y2", area.y + area.height);
    hLine.setAttribute("x1", area.x);
    hLine.setAttribute("x2", area.x + area.width);
    hLine.setAttribute("y1", y);
    hLine.setAttribute("y2", y);
    crosshair.style.display = "";
    setLines(crossText, labels);
    placeBox(
      crossLabel,
      crossBg,
      crossText,
      labels,
      { x: x, y: y },
      { x: area.x, y: area.y, width: area.width, height: area.height },
      true
    );
  }

  if (plotAreas.length) {
    root.addEventListener(moveEvent, updateCrosshair);
    root.addEventListener(leaveEvent, hideCrosshair);
  }

  var highlighted = root.querySelectorAll("[data-algraf-highlight]");
  var titled = [];
  var titles = root.querySelectorAll("title");
  for (var i = 0; i < titles.length; i++) {
    var title = titles[i];
    var parent = title.parentNode;
    if (!parent || parent === root) continue;
    var titleText = title.textContent || "";
    parent.setAttribute("data-title", titleText);
    parent.setAttribute("aria-label", titleText.replace(/\n/g, ", "));
    parent.setAttribute("tabindex", "0");
    parent.removeChild(title);
    pushUnique(titled, parent);
  }

  function pushUnique(list, el) {
    for (var i = 0; i < list.length; i++) {
      if (list[i] === el) return;
    }
    list.push(el);
  }

  function setHighlight(group) {
    for (var i = 0; i < highlighted.length; i++) {
      var el = highlighted[i];
      var value = el.getAttribute("data-algraf-highlight");
      el.style.opacity = group === null || value === group ? "" : "0.15";
    }
  }

  function showTooltip(el, point) {
    var data = el.getAttribute("data-title");
    if (!data) return;
    var labels = data.split("\n");
    tooltip.style.display = "";
    setLines(tooltipText, labels);
    placeBox(tooltip, tooltipBg, tooltipText, labels, point || elementPoint(el), svgBounds(), false);
  }

  function activate(el, evt) {
    if (el.hasAttribute("data-algraf-highlight")) {
      setHighlight(el.getAttribute("data-algraf-highlight"));
    }
    var point = evt ? eventPoint(evt) : null;
    showTooltip(el, point);
  }

  function deactivate(el) {
    if (el.hasAttribute("data-algraf-highlight")) setHighlight(null);
    tooltip.style.display = "none";
  }

  var interactive = [];
  for (var hi = 0; hi < highlighted.length; hi++) pushUnique(interactive, highlighted[hi]);
  for (var ti = 0; ti < titled.length; ti++) pushUnique(interactive, titled[ti]);

  for (var ii = 0; ii < interactive.length; ii++) {
    (function(el) {
      el.style.cursor = "pointer";
      el.addEventListener(enterEvent, function(evt) {
        activate(el, evt);
      });
      el.addEventListener(moveEvent, function(evt) {
        if (tooltip.style.display !== "none") showTooltip(el, eventPoint(evt));
      });
      el.addEventListener(leaveEvent, function() {
        deactivate(el);
      });
      el.addEventListener("focus", function() {
        activate(el, null);
      });
      el.addEventListener("blur", function() {
        deactivate(el);
      });
    })(interactive[ii]);
  }
})();"##;

pub(super) fn emit_document(
    scene: &RenderScene<'_>,
    interactive: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let RenderScene {
        ir,
        layout,
        legends,
        panels,
        theme,
    } = *scene;
    let width = ir.width as f64;
    let height = ir.height as f64;

    let mut w = SvgWriter::new();
    let aria = ir
        .alt
        .as_ref()
        .or(ir.title.as_ref())
        .map(|label| format!(" aria-label=\"{}\"", escape_attr(label)))
        .unwrap_or_default();
    w.line(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\" role=\"img\"{}>",
        num(width),
        num(height),
        num(width),
        num(height),
        aria,
    ));
    if let Some(title) = &ir.title {
        w.text_element("title", &[], title);
    }
    if let Some(desc) = chart_desc(ir) {
        w.text_element("desc", &[], &desc);
    }

    // Background.
    emit_rect_fill(
        &mut w,
        0.0,
        0.0,
        width,
        height,
        &theme.background,
        "algraf-background",
    );
    render_chart_text(&mut w, ir, width, height, layout, theme);

    let slots = panel_slots(layout, panels);
    for slot in &slots {
        let class = if slot.facet_index.is_some() {
            "algraf-plot-area algraf-facet-panel"
        } else {
            "algraf-plot-area"
        };
        emit_panel_background(
            &mut w,
            slot.plot.x,
            slot.plot.y,
            slot.plot.width,
            slot.plot.height,
            class,
            theme,
        );
    }

    // The chart body and guides are emitted through the shared mark sink so the
    // SVG and draw-list backends agree on coordinates and colors (spec §24.6).
    {
        let mut sink = SvgSink::new(&mut w);
        for slot in &slots {
            if let (Some(strip), Some(panel)) = (slot.strip, slot.panel) {
                guide::render_facet_label(
                    &mut sink,
                    slot.label.unwrap_or_default(),
                    strip,
                    &panel.theme,
                );
            }
        }
        paint_grid(&mut sink, &slots);
        paint_geometries(&mut sink, panels, diagnostics);
        paint_axes_and_legends(&mut sink, &slots, legends, layout, theme);
    }

    // Opt-in interactive runtime (spec §29.3). The only path by which a
    // `<script>` enters Algraf output; absent the opt-in the SVG is script-free.
    if interactive {
        w.line("<script type=\"application/ecmascript\"><![CDATA[");
        w.line(INTERACTIVE_RUNTIME);
        w.line("]]></script>");
    }

    w.line("</svg>");
    w.finish()
}

/// Draw gridlines behind the data marks, for every panel (spec §17.6, §16.16).
/// Shared by the SVG and draw-list backends.
pub(super) fn paint_grid(sink: &mut dyn MarkSink, slots: &[PanelSlot<'_>]) {
    for slot in slots {
        if let Some(panel) = slot.panel {
            if panel.scaled.is_polar() {
                guide::render_polar_grid(sink, &panel.scaled, &panel.guides, &panel.theme);
            } else if panel.guides.grid && !panel.scaled.is_spatial() {
                guide::render_grid(sink, &panel.scaled, panel.plot, &panel.theme);
            }
        }
    }
}

/// Draw the per-datum geometry marks of every layer in source order (spec
/// §18.3). Shared by the SVG and draw-list backends.
pub(super) fn paint_geometries(
    sink: &mut dyn MarkSink,
    panels: &[Panel<'_>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for panel in panels {
        if panel.clip_marks {
            sink.open_clip(panel.plot);
        }
        for geo in panel.geometries {
            crate::geom::render(
                sink,
                geo,
                crate::geom::GeometryRenderContext {
                    space: &panel.scaled,
                    table: panel.table,
                    rows: panel.rows.as_deref(),
                    plot: panel.plot,
                    theme: &panel.theme,
                    scales: &panel.scales,
                },
                diagnostics,
            );
        }
        if panel.clip_marks {
            sink.close_clip();
        }
    }
}

/// Draw axes (or polar labels) and legends above the data marks. Shared by the
/// SVG and draw-list backends.
pub(super) fn paint_axes_and_legends(
    sink: &mut dyn MarkSink,
    slots: &[PanelSlot<'_>],
    legends: &[crate::aes::Legend],
    layout: &Layout,
    theme: &Theme,
) {
    for slot in slots {
        if let Some(panel) = slot.panel {
            // Spatial spaces have no lat/lon axes (spec §16.15); polar spaces use
            // ring/spoke guides instead of Cartesian axes (§16.16).
            if panel.scaled.is_polar() {
                guide::render_polar_labels(sink, &panel.scaled, &panel.guides, &panel.theme);
            } else if panel.theme.axes && !panel.scaled.is_spatial() {
                guide::render_axes(
                    sink,
                    &panel.scaled,
                    panel.plot,
                    &panel.theme,
                    guide::AxisRenderOptions {
                        x_label_override: panel.guides.x_label.as_deref(),
                        y_label_override: panel.guides.y_label.as_deref(),
                        x_time_format: panel.guides.x_time_format.as_ref(),
                        y_time_format: panel.guides.y_time_format.as_ref(),
                        x_tick_label_angle: panel.guides.x_tick_label_angle,
                        y_tick_label_angle: panel.guides.y_tick_label_angle,
                        x_tick_label_rows: panel.guides.x_tick_label_rows,
                        y_tick_label_rows: panel.guides.y_tick_label_rows,
                    },
                );
            }
        }
    }
    if let Some(area) = layout.legend {
        guide::render_legends(sink, legends, area, theme);
    }
}

fn chart_desc(ir: &ChartIr) -> Option<String> {
    if let Some(description) = &ir.description {
        return Some(description.clone());
    }
    match (&ir.subtitle, &ir.caption) {
        (Some(subtitle), Some(caption)) => Some(format!("{subtitle}\n{caption}")),
        (Some(subtitle), None) => Some(subtitle.clone()),
        (None, Some(caption)) => Some(caption.clone()),
        (None, None) => None,
    }
}

fn render_chart_text(
    w: &mut SvgWriter,
    ir: &ChartIr,
    width: f64,
    height: f64,
    layout: &Layout,
    theme: &Theme,
) {
    let x = layout.plot.x;
    let mut y = 24.0;
    if let Some(title) = &ir.title {
        w.line(&format!(
            "<text class=\"algraf-title\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.plot_title.font_family),
            num(theme.plot_title.size),
            escape_attr(&theme.plot_title.fill),
            escape_text(title),
        ));
        y += theme.plot_title.size + 8.0;
    }
    if let Some(subtitle) = &ir.subtitle {
        w.line(&format!(
            "<text class=\"algraf-subtitle\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.plot_subtitle.font_family),
            num(theme.plot_subtitle.size),
            escape_attr(&theme.plot_subtitle.fill),
            escape_text(subtitle),
        ));
    }
    if let Some(caption) = &ir.caption {
        w.line(&format!(
            "<text class=\"algraf-caption\" x=\"{}\" y=\"{}\" text-anchor=\"end\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(width - 16.0),
            num(height - 12.0),
            escape_attr(&theme.plot_caption.font_family),
            num(theme.plot_caption.size),
            escape_attr(&theme.plot_caption.fill),
            escape_text(caption),
        ));
    }
}

fn emit_rect_fill(
    w: &mut SvgWriter,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: &str,
    class: &str,
) {
    w.empty_element(
        "rect",
        &[
            SvgAttr::new("class", class),
            SvgAttr::number("x", x),
            SvgAttr::number("y", y),
            SvgAttr::number("width", width),
            SvgAttr::number("height", height),
            SvgAttr::new("fill", color),
        ],
    );
}

fn emit_panel_background(
    w: &mut SvgWriter,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    class: &str,
    theme: &Theme,
) {
    let mut attrs = format!(
        "class=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"",
        escape_attr(class),
        num(x),
        num(y),
        num(width),
        num(height),
        escape_attr(&theme.panel_background.fill),
    );
    if let Some(stroke) = &theme.panel_background.stroke {
        attrs.push_str(&format!(
            " stroke=\"{}\" stroke-width=\"{}\"",
            escape_attr(stroke),
            num(theme.panel_background.stroke_width),
        ));
    }
    w.line(&format!("<rect {attrs} />"));
}
