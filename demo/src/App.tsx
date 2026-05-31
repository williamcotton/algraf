import React from "react";
import {
  AlertCircle,
  CheckCircle2,
  Code2,
  Database,
  LoaderCircle,
  Play,
  RefreshCw,
} from "lucide-react";

import {
  type AlgrafDiagnostic,
  type AlgrafRenderResult,
  type AlgrafRuntime,
  loadAlgrafRuntime,
} from "./algrafWasm";
import { AlgrafChart } from "./AlgrafChart";
import { AlgrafEditor } from "./AlgrafEditor";

interface DemoDataset {
  file: string;
  label: string;
  rows: number;
  columns: number;
  url: string;
}

interface ChartPreset {
  id: string;
  title: string;
  dataset: string;
  summary: string;
  source: string;
}

const DATASETS: Record<string, DemoDataset> = {
  penguins: {
    file: "penguins.csv",
    label: "Palmer penguins",
    rows: 344,
    columns: 8,
    url: "/data/penguins.csv",
  },
  gapminder: {
    file: "gapminder.csv",
    label: "Gapminder",
    rows: 1704,
    columns: 6,
    url: "/data/gapminder.csv",
  },
  iris: {
    file: "iris.csv",
    label: "Iris flowers",
    rows: 150,
    columns: 5,
    url: "/data/iris.csv",
  },
  stocks: {
    file: "stocks.csv",
    label: "Tech stocks",
    rows: 559,
    columns: 3,
    url: "/data/stocks.csv",
  },
  weather: {
    file: "seattle-weather.csv",
    label: "Seattle weather",
    rows: 1461,
    columns: 6,
    url: "/data/seattle-weather.csv",
  },
};

const CHART_PRESETS: ChartPreset[] = [
  {
    id: "penguins-morphology",
    title: "Penguins",
    dataset: "penguins",
    summary: "Scatter, trend, tooltips, highlight",
    source: `Chart(data: "penguins.csv", width: 820, height: 520, title: "Palmer penguin morphology") {
    Theme(name: "minimal")
    Scale(fill: species, palette: "accent")
    Scale(size: bill_length_mm, range: [2.5, 8.5], label: "Bill length (mm)")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(
            fill: species,
            shape: island,
            size: bill_length_mm,
            alpha: 0.68,
            tooltip: [species, island, sex, bill_length_mm, flipper_length_mm, body_mass_g],
            highlight: species
        )
        Smooth(method: "lm", stroke: species, strokeWidth: 2.4)
    }
}
`,
  },
  {
    id: "gapminder-bubbles",
    title: "Gapminder",
    dataset: "gapminder",
    summary: "Log scale, bubbles, 1,704 rows",
    source: `Chart(data: "gapminder.csv", width: 840, height: 520, title: "Wealth, health, and population") {
    Theme(name: "minimal")
    Scale(axis: x, type: "log10")
    Scale(fill: continent, palette: "default")
    Scale(size: pop, range: [1.5, 16], label: "Population")
    Guide(axis: x, label: "GDP per capita")
    Guide(axis: y, label: "Life expectancy")

    Space(gdpPercap * lifeExp) {
        Point(
            fill: continent,
            size: pop,
            alpha: 0.36,
            tooltip: [country, year, continent, lifeExp, gdpPercap, pop],
            highlight: continent
        )
    }
}
`,
  },
  {
    id: "iris-petals",
    title: "Iris",
    dataset: "iris",
    summary: "Quoted fields, shape and color",
    source: `Chart(data: "iris.csv", width: 760, height: 500, title: "Iris petal measurements") {
    Theme(name: "minimal")
    Scale(fill: \`class\`, palette: "accent")
    Guide(axis: x, label: "Petal length")
    Guide(axis: y, label: "Petal width")

    Space(\`petal length\` * \`petal width\`) {
        Point(
            fill: \`class\`,
            shape: \`class\`,
            size: \`sepal width\`,
            alpha: 0.72,
            tooltip: [\`class\`, \`sepal length\`, \`sepal width\`, \`petal length\`, \`petal width\`],
            highlight: \`class\`
        )
    }
}
`,
  },
  {
    id: "stocks-lines",
    title: "Stocks",
    dataset: "stocks",
    summary: "Temporal parse and grouped lines",
    source: `Chart(data: "stocks.csv", width: 840, height: 500, title: "Stock prices by symbol") {
    Theme(name: "minimal")
    Parse(column: date, as: "date", format: "%b %-d %Y")
    Guide(axis: x, label: "Month", timeFormat: "%Y")
    Guide(axis: y, label: "Price")

    Space(date * price) {
        Line(stroke: symbol, strokeWidth: 2.1)
        Point(
            fill: symbol,
            size: 2.2,
            alpha: 0.5,
            tooltip: [symbol, date, price],
            highlight: symbol
        )
    }
}
`,
  },
  {
    id: "weather-range",
    title: "Weather",
    dataset: "weather",
    summary: "Ribbon, line, daily categories",
    source: `Chart(data: "seattle-weather.csv", width: 840, height: 500, title: "Seattle daily temperature range") {
    Theme(name: "minimal")
    Guide(axis: x, label: "Date", timeFormat: "%Y")
    Guide(axis: y, label: "Temperature (C)")

    Space(date * temp_max) {
        Ribbon(ymin: temp_min, ymax: temp_max, fill: "#9ecae1", alpha: 0.36)
        Line(stroke: "#d95f02", strokeWidth: 1.8)
        Point(
            fill: weather,
            size: 1.8,
            alpha: 0.45,
            tooltip: [date, weather, temp_min, temp_max, precipitation, wind],
            highlight: weather
        )
    }
}
`,
  },
];

const DEFAULT_PRESET = CHART_PRESETS[0];

type LoadState = "loading" | "ready" | "error";

interface PreviewStats {
  marks: number;
  groups: number;
}

interface RenderSnapshot {
  source: string;
  dataText: string;
  dataFile: string;
  result: AlgrafRenderResult;
}

export function App(): React.ReactElement {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [runtimeState, setRuntimeState] = React.useState<LoadState>("loading");
  const [selectedPresetId, setSelectedPresetId] = React.useState(DEFAULT_PRESET.id);
  const [source, setSource] = React.useState(DEFAULT_PRESET.source);
  const [dataText, setDataText] = React.useState("");
  const [dataState, setDataState] = React.useState<LoadState>("loading");
  const [dataRevision, setDataRevision] = React.useState(0);
  const [renderSnapshot, setRenderSnapshot] = React.useState<RenderSnapshot | null>(null);
  const [rendering, setRendering] = React.useState(false);
  const [runtimeError, setRuntimeError] = React.useState<string | null>(null);
  const [dataError, setDataError] = React.useState<string | null>(null);
  const selectedPreset = CHART_PRESETS.find((preset) => preset.id === selectedPresetId) ?? DEFAULT_PRESET;
  const selectedDataset = DATASETS[selectedPreset.dataset] ?? DATASETS.penguins;

  React.useEffect(() => {
    let cancelled = false;
    setRuntimeState("loading");
    loadAlgrafRuntime()
      .then((loaded) => {
        if (cancelled) return;
        setRuntime(loaded);
        setRuntimeState("ready");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setRuntimeError(errorMessage(err));
        setRuntimeState("error");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    setDataState("loading");
    setDataError(null);
    setDataText("");

    fetch(selectedDataset.url)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`failed to fetch ${selectedDataset.url}: ${response.status}`);
        }
        return response.text();
      })
      .then((text) => {
        if (cancelled) return;
        setDataText(text);
        setDataState("ready");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setDataError(errorMessage(err));
        setDataState("error");
      });

    return () => {
      cancelled = true;
    };
  }, [dataRevision, selectedDataset.url]);

  const renderCurrent = React.useCallback(() => {
    if (!runtime || dataState !== "ready") {
      return;
    }

    const renderSource = source;
    const renderDataText = dataText;
    setRendering(true);
    window.setTimeout(() => {
      try {
        setRenderSnapshot({
          source: renderSource,
          dataText: renderDataText,
          dataFile: selectedDataset.file,
          result: runtime.render(renderSource, { [selectedDataset.file]: renderDataText }),
        });
      } catch (err: unknown) {
        setRenderSnapshot({
          source: renderSource,
          dataText: renderDataText,
          dataFile: selectedDataset.file,
          result: {
            svg: null,
            sidecar: null,
            diagnostics: [],
            error: errorMessage(err),
          },
        });
      } finally {
        setRendering(false);
      }
    }, 0);
  }, [dataState, dataText, runtime, selectedDataset.file, source]);

  React.useEffect(() => {
    if (!runtime || dataState !== "ready") {
      return;
    }

    const timer = window.setTimeout(() => {
      renderCurrent();
    }, 260);

    return () => window.clearTimeout(timer);
  }, [dataState, renderCurrent, runtime]);

  const selectPreset = React.useCallback((preset: ChartPreset) => {
    setSelectedPresetId(preset.id);
    setSource(preset.source);
    setRenderSnapshot(null);
    setDataRevision((revision) => revision + 1);
  }, []);

  const result = renderSnapshot?.result ?? null;
  const diagnosticsAreCurrent = Boolean(
    renderSnapshot &&
      renderSnapshot.source === source &&
      renderSnapshot.dataText === dataText &&
      renderSnapshot.dataFile === selectedDataset.file,
  );
  const diagnostics = diagnosticsAreCurrent ? (result?.diagnostics ?? []) : [];
  const currentError = diagnosticsAreCurrent ? (result?.error ?? null) : null;
  const hasErrors = diagnostics.some((diagnostic) => diagnostic.severity === "error") || Boolean(currentError);
  const stats = React.useMemo(() => previewStats(result?.sidecar), [result?.sidecar]);
  const dataRows = React.useMemo(() => estimateRows(dataText, selectedDataset.file), [dataText, selectedDataset.file]);
  const editorFiles = React.useMemo(() => ({ [selectedDataset.file]: dataText }), [dataText, selectedDataset.file]);

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <h1>Algraf WASM Playground</h1>
          <p>{selectedDataset.label} served from the dev server and rendered in the browser runtime.</p>
        </div>
        <div className="header-actions">
          <StatusBadge state={runtimeState} label="WASM" error={runtimeError} />
          <StatusBadge state={dataState} label="Data" error={dataError} />
          <button className="icon-button" type="button" disabled={!runtime || dataState !== "ready"} onClick={renderCurrent}>
            <Play size={16} aria-hidden="true" />
            Render
          </button>
        </div>
      </header>

      <section className="preset-strip" aria-label="Chart presets">
        {CHART_PRESETS.map((preset) => {
          const dataset = DATASETS[preset.dataset];
          const active = preset.id === selectedPreset.id;
          return (
            <button
              className={`preset-card ${active ? "preset-card-active" : ""}`}
              key={preset.id}
              type="button"
              onClick={() => selectPreset(preset)}
            >
              <span className="preset-title">{preset.title}</span>
              <span className="preset-meta">
                {dataset.label} - {dataset.rows.toLocaleString()} rows
              </span>
              <span className="preset-summary">{preset.summary}</span>
            </button>
          );
        })}
      </section>

      <section className="workspace-grid">
        <div className="pane editor-pane">
          <PaneHeader icon={<Code2 size={17} aria-hidden="true" />} title="Algraf" detail={`${source.length} bytes`} />
          <AlgrafEditor
            diagnostics={diagnostics}
            files={editorFiles}
            onChange={setSource}
            runtime={runtime}
            value={source}
          />
        </div>

        <div className="pane data-pane">
          <PaneHeader
            icon={<Database size={17} aria-hidden="true" />}
            title={selectedDataset.file}
            detail={`${dataRows ?? selectedDataset.rows} rows, ${selectedDataset.columns} cols, ${formatBytes(dataText.length)}`}
            action={
              <button className="compact-button" type="button" onClick={() => setDataRevision((revision) => revision + 1)}>
                <RefreshCw size={15} aria-hidden="true" />
                Reload
              </button>
            }
          />
          <textarea
            aria-label={`${selectedDataset.label} data`}
            className="data-input"
            spellCheck={false}
            value={dataText}
            onChange={(event) => setDataText(event.target.value)}
          />
        </div>

        <div className="pane preview-pane">
          <PaneHeader
            icon={rendering ? <LoaderCircle className="spin" size={17} aria-hidden="true" /> : <CheckCircle2 size={17} aria-hidden="true" />}
            title="Preview"
            detail={stats ? `${stats.marks} marks, ${stats.groups} groups` : "Awaiting render"}
          />
          <div className="preview-stage">
            {result?.svg ? (
              <div className="chart-output">
                <AlgrafChart sidecar={result.sidecar} svg={result.svg} />
              </div>
            ) : (
              <div className="empty-preview">
                <AlertCircle size={24} aria-hidden="true" />
                <span>{result?.error ?? runtimeError ?? dataError ?? "Loading runtime and data"}</span>
              </div>
            )}
          </div>
          <DiagnosticsPanel
            diagnostics={diagnostics}
            error={currentError}
            hasErrors={hasErrors}
            stale={Boolean(renderSnapshot && !diagnosticsAreCurrent)}
          />
        </div>
      </section>
    </main>
  );
}

function StatusBadge({ state, label, error }: { state: LoadState; label: string; error: string | null }): React.ReactElement {
  const statusLabel = state === "ready" ? "ready" : state === "loading" ? "loading" : "error";
  return (
    <span className={`status-badge status-${state}`} title={error ?? undefined}>
      {state === "loading" ? <LoaderCircle className="spin" size={14} aria-hidden="true" /> : null}
      {state === "ready" ? <CheckCircle2 size={14} aria-hidden="true" /> : null}
      {state === "error" ? <AlertCircle size={14} aria-hidden="true" /> : null}
      {label}: {statusLabel}
    </span>
  );
}

function PaneHeader({
  icon,
  title,
  detail,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  detail?: string;
  action?: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="pane-header">
      <div className="pane-title">
        {icon}
        <span>{title}</span>
      </div>
      <div className="pane-detail">{detail}</div>
      {action}
    </div>
  );
}

function DiagnosticsPanel({
  diagnostics,
  error,
  hasErrors,
  stale,
}: {
  diagnostics: AlgrafDiagnostic[];
  error: string | null;
  hasErrors: boolean;
  stale: boolean;
}): React.ReactElement {
  if (stale) {
    return (
      <div className="diagnostics diagnostics-pending">
        <LoaderCircle className="spin" size={16} aria-hidden="true" />
        Diagnostics updating
      </div>
    );
  }

  if (!error && diagnostics.length === 0) {
    return (
      <div className="diagnostics diagnostics-ok">
        <CheckCircle2 size={16} aria-hidden="true" />
        No diagnostics
      </div>
    );
  }

  return (
    <div className={`diagnostics ${hasErrors ? "diagnostics-error" : "diagnostics-warning"}`}>
      {error ? (
        <div className="diagnostic-row">
          <AlertCircle size={16} aria-hidden="true" />
          <span className="diagnostic-code">Runtime</span>
          <span>{error}</span>
        </div>
      ) : null}
      {diagnostics.map((diagnostic, index) => (
        <div className="diagnostic-row" key={`${diagnostic.code}-${diagnostic.span.start}-${index}`}>
          <AlertCircle size={16} aria-hidden="true" />
          <span className="diagnostic-code">{diagnostic.code}</span>
          <span className="diagnostic-message">
            {diagnostic.message} <span className="diagnostic-span">[{diagnostic.span.start}, {diagnostic.span.end})</span>
            {diagnostic.help ? <span className="diagnostic-help">{diagnostic.help}</span> : null}
            {diagnostic.related?.map((related, relatedIndex) => (
              <span className="diagnostic-related" key={`${related.span.start}-${relatedIndex}`}>
                {related.message} <span className="diagnostic-span">[{related.span.start}, {related.span.end})</span>
              </span>
            ))}
          </span>
        </div>
      ))}
    </div>
  );
}

function previewStats(sidecar: string | null | undefined): PreviewStats | null {
  if (!sidecar) {
    return null;
  }
  try {
    const parsed = JSON.parse(sidecar) as {
      marks?: unknown[];
      groups?: Record<string, unknown[]>;
    };
    return {
      marks: parsed.marks?.length ?? 0,
      groups: parsed.groups ? Object.keys(parsed.groups).length : 0,
    };
  } catch {
    return null;
  }
}

function estimateRows(text: string, file: string): number | null {
  if (!text.trim()) {
    return null;
  }

  if (file.endsWith(".json")) {
    try {
      const parsed = JSON.parse(text) as unknown;
      return Array.isArray(parsed) ? parsed.length : null;
    } catch {
      return null;
    }
  }

  const lines = text.split(/\r?\n/).filter((line) => line.trim().length > 0);
  if (lines.length === 0) {
    return null;
  }
  return Math.max(0, lines.length - 1);
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
