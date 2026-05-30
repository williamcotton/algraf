import React from "react";

interface InteractionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface InteractionAxes {
  x?: InteractionAxis;
  y?: InteractionAxis;
}

interface InteractionAxis {
  scale: string;
  domain: Array<number | string>;
  range: [number, number];
  format: string;
  label: string;
  paddingInner?: number;
  paddingOuter?: number;
  bandwidth?: number;
  innerDomain?: string[];
}

interface InteractionPlot {
  id: string;
  plot_rect: InteractionRect;
  axes: InteractionAxes;
}

export interface InteractionMark {
  id: string;
  plot: string;
  x_px: number;
  y_px: number;
  groups: Record<string, string>;
  tooltip: TooltipRow[];
}

interface TooltipRow {
  label: string;
  value: string;
}

interface InteractionMetadata {
  version: 1;
  plot_rect: InteractionRect;
  axes: InteractionAxes;
  marks: InteractionMark[];
  groups: Record<string, string[]>;
  plots: InteractionPlot[];
}

interface SvgSize {
  width: number;
  height: number;
}

interface HoverState {
  x: number;
  y: number;
  plot: InteractionPlot;
  mark: InteractionMark | null;
}

interface GroupValue {
  key: string;
  value: string;
}

export interface AlgrafChartProps {
  svg: string;
  sidecar: string | null;
  onHoverMark?: (mark: InteractionMark | null) => void;
}

export function AlgrafChart({ svg, sidecar, onHoverMark }: AlgrafChartProps): React.ReactElement {
  const svgHostRef = React.useRef<HTMLDivElement | null>(null);
  const metadata = React.useMemo(() => parseInteractionMetadata(sidecar), [sidecar]);
  const size = React.useMemo(() => readSvgSize(svg, metadata), [metadata, svg]);
  const [hover, setHover] = React.useState<HoverState | null>(null);
  const activeGroup = hover?.mark ? firstGroupValue(hover.mark.groups) : null;
  const tooltip = hover?.mark && hover.mark.tooltip.length > 0 ? hover.mark : null;
  const readouts = hover ? crosshairReadouts(hover) : { x: null, y: null };

  React.useEffect(() => {
    onHoverMark?.(hover?.mark ?? null);
  }, [hover?.mark, onHoverMark]);

  React.useEffect(() => {
    const host = svgHostRef.current;
    if (!host) {
      return;
    }

    const marks = Array.from(host.querySelectorAll("[data-algraf-highlight]"));
    for (const mark of marks) {
      mark.classList.remove("algraf-svg-mark-active", "algraf-svg-mark-dimmed");
    }

    if (!activeGroup) {
      return;
    }

    for (const mark of marks) {
      if (mark.getAttribute("data-algraf-highlight") === activeGroup.value) {
        mark.classList.add("algraf-svg-mark-active");
      } else {
        mark.classList.add("algraf-svg-mark-dimmed");
      }
    }

    return () => {
      for (const mark of marks) {
        mark.classList.remove("algraf-svg-mark-active", "algraf-svg-mark-dimmed");
      }
    };
  }, [activeGroup?.value, svg]);

  const handlePointerMove = React.useCallback(
    (event: React.PointerEvent<SVGSVGElement>) => {
      if (!metadata) {
        return;
      }

      const point = pointerToSvgPoint(event, size);
      const plot = plotAtPoint(metadata, point.x, point.y);
      if (!plot) {
        setHover(null);
        return;
      }

      const rect = plot.plot_rect;
      const x = clamp(point.x, rect.x, rect.x + rect.width);
      const y = clamp(point.y, rect.y, rect.y + rect.height);
      setHover({
        x,
        y,
        plot,
        mark: nearestMark(metadata, plot.id, x, y),
      });
    },
    [metadata, size],
  );

  return (
    <div className="algraf-chart">
      <div className="algraf-chart-svg" ref={svgHostRef} dangerouslySetInnerHTML={{ __html: svg }} />
      {metadata ? (
        <svg
          aria-hidden="true"
          className="algraf-chart-overlay"
          focusable="false"
          onPointerLeave={() => setHover(null)}
          onPointerMove={handlePointerMove}
          preserveAspectRatio="xMinYMin meet"
          viewBox={`0 0 ${size.width} ${size.height}`}
        >
          {hover ? <Crosshair hover={hover} /> : null}
          {activeGroup
            ? metadata.marks
                .filter((mark) => mark.groups[activeGroup.key] === activeGroup.value)
                .map((mark) => <circle className="algraf-mark-halo" cx={mark.x_px} cy={mark.y_px} key={mark.id} r="8" />)
            : null}
          {hover?.mark ? <circle className="algraf-mark-focus" cx={hover.mark.x_px} cy={hover.mark.y_px} r="9" /> : null}
        </svg>
      ) : null}
      {readouts.x ? (
        <div
          className="algraf-crosshair-readout algraf-crosshair-readout-x"
          style={overlayPosition(readouts.x.x, readouts.x.y, size)}
        >
          {readouts.x.label}: {readouts.x.value}
        </div>
      ) : null}
      {readouts.y ? (
        <div
          className="algraf-crosshair-readout algraf-crosshair-readout-y"
          style={overlayPosition(readouts.y.x, readouts.y.y, size)}
        >
          {readouts.y.label}: {readouts.y.value}
        </div>
      ) : null}
      {tooltip ? (
        <div
          className={`algraf-tooltip ${tooltip.x_px > size.width * 0.66 ? "algraf-tooltip-left" : "algraf-tooltip-right"}`}
          style={overlayPosition(tooltip.x_px, clamp(tooltip.y_px, 72, size.height - 72), size)}
        >
          {tooltip.tooltip.map((row) => (
            <div className="algraf-tooltip-row" key={row.label}>
              <span>{row.label}</span>
              <strong>{row.value}</strong>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Crosshair({ hover }: { hover: HoverState }): React.ReactElement {
  const rect = hover.plot.plot_rect;
  const right = rect.x + rect.width;
  const bottom = rect.y + rect.height;
  return (
    <g className="algraf-crosshair">
      <line x1={hover.x} x2={hover.x} y1={rect.y} y2={bottom} />
      <line x1={rect.x} x2={right} y1={hover.y} y2={hover.y} />
    </g>
  );
}

function parseInteractionMetadata(sidecar: string | null): InteractionMetadata | null {
  if (!sidecar) {
    return null;
  }

  try {
    const parsed = JSON.parse(sidecar) as unknown;
    if (!isRecord(parsed) || parsed.version !== 1) {
      return null;
    }

    const plotRect = toRect(parsed.plot_rect);
    const axes = toAxes(parsed.axes);
    if (!plotRect || !axes) {
      return null;
    }

    const marks = Array.isArray(parsed.marks) ? parsed.marks.map(toMark).filter(isPresent) : [];
    const plots = Array.isArray(parsed.plots) ? parsed.plots.map(toPlot).filter(isPresent) : [];

    return {
      version: 1,
      plot_rect: plotRect,
      axes,
      marks,
      groups: toGroups(parsed.groups),
      plots: plots.length > 0 ? plots : [{ id: "plot0", plot_rect: plotRect, axes }],
    };
  } catch {
    return null;
  }
}

function readSvgSize(svg: string, metadata: InteractionMetadata | null): SvgSize {
  if (typeof DOMParser !== "undefined") {
    const parsed = new DOMParser().parseFromString(svg, "image/svg+xml");
    const root = parsed.documentElement;
    const viewBox = root.getAttribute("viewBox")?.split(/\s+/).map(Number);
    if (viewBox?.length === 4 && isFiniteNumber(viewBox[2]) && isFiniteNumber(viewBox[3])) {
      return { width: viewBox[2], height: viewBox[3] };
    }

    const width = parseFloat(root.getAttribute("width") ?? "");
    const height = parseFloat(root.getAttribute("height") ?? "");
    if (isFiniteNumber(width) && isFiniteNumber(height)) {
      return { width, height };
    }
  }

  const rect = metadata?.plot_rect;
  if (rect) {
    return {
      width: Math.max(1, rect.x + rect.width),
      height: Math.max(1, rect.y + rect.height),
    };
  }

  return { width: 1, height: 1 };
}

function toPlot(value: unknown): InteractionPlot | null {
  if (!isRecord(value)) {
    return null;
  }
  const id = typeof value.id === "string" ? value.id : null;
  const plotRect = toRect(value.plot_rect);
  const axes = toAxes(value.axes);
  if (!id || !plotRect || !axes) {
    return null;
  }
  return { id, plot_rect: plotRect, axes };
}

function toRect(value: unknown): InteractionRect | null {
  if (!isRecord(value)) {
    return null;
  }
  const x = toNumber(value.x);
  const y = toNumber(value.y);
  const width = toNumber(value.width);
  const height = toNumber(value.height);
  if (x === null || y === null || width === null || height === null) {
    return null;
  }
  return { x, y, width, height };
}

function toAxes(value: unknown): InteractionAxes | null {
  if (!isRecord(value)) {
    return {};
  }
  const axes: InteractionAxes = {};
  const x = toAxis(value.x);
  const y = toAxis(value.y);
  if (x) {
    axes.x = x;
  }
  if (y) {
    axes.y = y;
  }
  return axes;
}

function toAxis(value: unknown): InteractionAxis | null {
  if (!isRecord(value)) {
    return null;
  }
  const range = toNumberPair(value.range);
  const domain = Array.isArray(value.domain) ? value.domain.filter(isNumberOrString) : [];
  if (!range || domain.length === 0) {
    return null;
  }
  return {
    scale: typeof value.scale === "string" ? value.scale : "linear",
    domain,
    range,
    format: typeof value.format === "string" ? value.format : "",
    label: typeof value.label === "string" ? value.label : "",
    paddingInner: toOptionalNumber(value.paddingInner),
    paddingOuter: toOptionalNumber(value.paddingOuter),
    bandwidth: toOptionalNumber(value.bandwidth),
    innerDomain: Array.isArray(value.innerDomain) ? value.innerDomain.filter((item): item is string => typeof item === "string") : [],
  };
}

function toMark(value: unknown): InteractionMark | null {
  if (!isRecord(value)) {
    return null;
  }
  const id = typeof value.id === "string" ? value.id : null;
  const plot = typeof value.plot === "string" ? value.plot : null;
  const x = toNumber(value.x_px);
  const y = toNumber(value.y_px);
  if (!id || !plot || x === null || y === null) {
    return null;
  }
  return {
    id,
    plot,
    x_px: x,
    y_px: y,
    groups: toGroupValues(value.groups),
    tooltip: Array.isArray(value.tooltip) ? value.tooltip.map(toTooltipRow).filter(isPresent) : [],
  };
}

function toTooltipRow(value: unknown): TooltipRow | null {
  if (!isRecord(value) || typeof value.label !== "string" || typeof value.value !== "string") {
    return null;
  }
  return {
    label: value.label,
    value: value.value,
  };
}

function toGroups(value: unknown): Record<string, string[]> {
  if (!isRecord(value)) {
    return {};
  }
  const groups: Record<string, string[]> = {};
  for (const [key, values] of Object.entries(value)) {
    if (Array.isArray(values)) {
      groups[key] = values.filter((item): item is string => typeof item === "string");
    }
  }
  return groups;
}

function toGroupValues(value: unknown): Record<string, string> {
  if (!isRecord(value)) {
    return {};
  }
  const groups: Record<string, string> = {};
  for (const [key, groupValue] of Object.entries(value)) {
    if (typeof groupValue === "string") {
      groups[key] = groupValue;
    }
  }
  return groups;
}

function pointerToSvgPoint(event: React.PointerEvent<SVGSVGElement>, size: SvgSize): { x: number; y: number } {
  const rect = event.currentTarget.getBoundingClientRect();
  return {
    x: ((event.clientX - rect.left) / rect.width) * size.width,
    y: ((event.clientY - rect.top) / rect.height) * size.height,
  };
}

function plotAtPoint(metadata: InteractionMetadata, x: number, y: number): InteractionPlot | null {
  return metadata.plots.find((plot) => rectContains(plot.plot_rect, x, y)) ?? null;
}

function rectContains(rect: InteractionRect, x: number, y: number): boolean {
  return x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
}

function nearestMark(metadata: InteractionMetadata, plotId: string, x: number, y: number): InteractionMark | null {
  let nearest: InteractionMark | null = null;
  let nearestDistance = Number.POSITIVE_INFINITY;

  for (const mark of metadata.marks) {
    if (mark.plot !== plotId) {
      continue;
    }
    const distance = (mark.x_px - x) ** 2 + (mark.y_px - y) ** 2;
    if (distance < nearestDistance) {
      nearest = mark;
      nearestDistance = distance;
    }
  }

  return nearestDistance <= 20 ** 2 ? nearest : null;
}

function crosshairReadouts(hover: HoverState): {
  x: { label: string; value: string; x: number; y: number } | null;
  y: { label: string; value: string; x: number; y: number } | null;
} {
  const rect = hover.plot.plot_rect;
  const xValue = hover.plot.axes.x ? invertAxis(hover.plot.axes.x, hover.x) : null;
  const yValue = hover.plot.axes.y ? invertAxis(hover.plot.axes.y, hover.y) : null;
  return {
    x: xValue
      ? {
          label: hover.plot.axes.x?.label || "x",
          value: xValue,
          x: hover.x,
          y: rect.y + 8,
        }
      : null,
    y: yValue
      ? {
          label: hover.plot.axes.y?.label || "y",
          value: yValue,
          x: rect.x + 8,
          y: hover.y,
        }
      : null,
  };
}

function invertAxis(axis: InteractionAxis, pixel: number): string | null {
  if (axis.scale === "band" || axis.scale === "nested-band") {
    return nearestBandValue(axis, pixel);
  }

  const domain = toNumberPair(axis.domain);
  if (!domain) {
    return null;
  }

  const transformed = interpolate(pixel, axis.range, transformDomain(axis.scale, domain));
  const value = untransformValue(axis.scale, transformed);
  if (!isFiniteNumber(value)) {
    return null;
  }

  if (axis.scale === "time") {
    return formatTime(value);
  }
  return formatNumber(value);
}

function transformDomain(scale: string, domain: [number, number]): [number, number] {
  if (scale === "log10") {
    return [Math.log10(domain[0]), Math.log10(domain[1])];
  }
  if (scale === "sqrt") {
    return [Math.sqrt(Math.max(0, domain[0])), Math.sqrt(Math.max(0, domain[1]))];
  }
  return domain;
}

function untransformValue(scale: string, value: number): number {
  if (scale === "log10") {
    return 10 ** value;
  }
  if (scale === "sqrt") {
    return value ** 2;
  }
  return value;
}

function interpolate(pixel: number, range: [number, number], domain: [number, number]): number {
  const t = (pixel - range[0]) / (range[1] - range[0]);
  return domain[0] + t * (domain[1] - domain[0]);
}

function nearestBandValue(axis: InteractionAxis, pixel: number): string | null {
  const domain = axis.domain.filter((value): value is string => typeof value === "string");
  if (domain.length === 0) {
    return null;
  }

  const paddingInner = axis.paddingInner ?? 0.2;
  const paddingOuter = axis.paddingOuter ?? 0.1;
  const step = (axis.range[1] - axis.range[0]) / (domain.length - paddingInner + 2 * paddingOuter);
  const width = Math.abs(step * (1 - paddingInner));
  let nearest = domain[0];
  let nearestDistance = Number.POSITIVE_INFINITY;

  domain.forEach((category, index) => {
    const start = axis.range[0] + paddingOuter * step + index * step;
    const center = step >= 0 ? start + width / 2 : start - width / 2;
    const distance = Math.abs(center - pixel);
    if (distance < nearestDistance) {
      nearest = category;
      nearestDistance = distance;
    }
  });

  return nearest;
}

function firstGroupValue(groups: Record<string, string>): GroupValue | null {
  const [first] = Object.entries(groups);
  return first ? { key: first[0], value: first[1] } : null;
}

function overlayPosition(x: number, y: number, size: SvgSize): React.CSSProperties {
  return {
    left: `${(x / size.width) * 100}%`,
    top: `${(y / size.height) * 100}%`,
  };
}

function formatNumber(value: number): string {
  const abs = Math.abs(value);
  if (abs !== 0 && abs < 0.01) {
    return trimNumber(value.toPrecision(3));
  }
  if (abs >= 1000) {
    return trimNumber(value.toFixed(0));
  }
  if (abs >= 100) {
    return trimNumber(value.toFixed(1));
  }
  return trimNumber(value.toFixed(2));
}

function formatTime(micros: number): string {
  const date = new Date(micros / 1000);
  return Number.isNaN(date.getTime()) ? formatNumber(micros) : date.toISOString().replace(".000Z", "Z");
}

function trimNumber(value: string): string {
  return value.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
}

function toNumberPair(value: unknown): [number, number] | null {
  if (!Array.isArray(value) || value.length < 2) {
    return null;
  }
  const first = toNumber(value[0]);
  const second = toNumber(value[1]);
  return first === null || second === null ? null : [first, second];
}

function toOptionalNumber(value: unknown): number | undefined {
  return toNumber(value) ?? undefined;
}

function toNumber(value: unknown): number | null {
  return typeof value === "number" && isFiniteNumber(value) ? value : null;
}

function isNumberOrString(value: unknown): value is number | string {
  return typeof value === "string" || (typeof value === "number" && isFiniteNumber(value));
}

function isFiniteNumber(value: number): boolean {
  return Number.isFinite(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isPresent<T>(value: T | null): value is T {
  return value !== null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
