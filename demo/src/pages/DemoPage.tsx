import React from "react";
import {
  AlertCircle,
  CheckCircle2,
  Code2,
  Database,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";

import {
  type AlgrafDiagnostic,
  type AlgrafRenderResult,
  type AlgrafRuntime,
  loadAlgrafRuntime,
} from "../algrafWasm";
import { AlgrafChart } from "../AlgrafChart";
import { AlgrafEditor } from "../AlgrafEditor";
import { publicAssetUrl } from "../publicAssets";

interface DemoDataFile {
  file: string;
  label: string;
  rows: number;
  columns: number;
  url: string;
}

interface DemoDataset extends DemoDataFile {
  auxiliaryFiles?: DemoDataFile[];
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
    url: publicAssetUrl("data/penguins.csv"),
  },
  gapminder: {
    file: "gapminder.csv",
    label: "Gapminder",
    rows: 1704,
    columns: 6,
    url: publicAssetUrl("data/gapminder.csv"),
  },
  iris: {
    file: "iris.csv",
    label: "Iris flowers",
    rows: 150,
    columns: 5,
    url: publicAssetUrl("data/iris.csv"),
  },
  stocks: {
    file: "stocks.csv",
    label: "Tech stocks",
    rows: 559,
    columns: 3,
    url: publicAssetUrl("data/stocks.csv"),
  },
  weather: {
    file: "seattle-weather.csv",
    label: "Seattle weather",
    rows: 1461,
    columns: 6,
    url: publicAssetUrl("data/seattle-weather.csv"),
  },
  astronauts: {
    file: "astronauts.csv",
    label: "Astronaut ages",
    rows: 564,
    columns: 2,
    url: publicAssetUrl("data/astronauts.csv"),
  },
  minard: {
    file: "minard_troops.csv",
    label: "Minard campaign",
    rows: 50,
    columns: 6,
    url: publicAssetUrl("data/minard_troops.csv"),
    auxiliaryFiles: [
      {
        file: "minard_cities.csv",
        label: "Minard cities",
        rows: 19,
        columns: 4,
        url: publicAssetUrl("data/minard_cities.csv"),
      },
    ],
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
    Scale(size: bill_length_mm, range: [2.5, 8.5], breaks: [35, 40, 45, 50, 55, 60], label: "Bill length (mm)")
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
    Scale(size: pop, range: [1.5, 16], breaks: [100000000, 300000000, 600000000, 900000000, 1200000000],
          labels: ["100M", "300M", "600M", "900M", "1.2B"], label: "Population")
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
  {
    id: "astronaut-ages",
    title: "Astronauts",
    dataset: "astronauts",
    summary: "Histogram, blend, annotations",
    source: `Chart(
    data: "astronauts.csv",
    width: 760,
    height: 460,
    title: "How old are astronauts on their most recent mission?",
    subtitle: "Age of astronauts when they were selected and when they were sent on their mission",
) {
    Theme(
        name: "minimal",
        plotBackground: "#EBEBEB",
        gridMajor: Line(stroke: "#FFFFFF", strokeWidth: 1),
    )
    Scale(axis: x, domain: [20, 80])
    Scale(axis: y, domain: [0, 69])
    Scale(
        fill: series,
        range: ["selection_age" => "#beaed4", "mission_age" => "#7fc97f"],
        labels: ["selection_age" => "Age at selection", "mission_age" => "Age at mission"],
        label: "",
    )
    Guide(axis: x, label: "Age of astronaut (years)")
    Guide(axis: y, label: "count")

    Space((mission_age + selection_age)) {
        Histogram(binWidth: 1, alpha: 0.8, stroke: "#000000")
        VLine(x: 34, stroke: "#000000", strokeWidth: 1, dash: "dotted")
        VLine(x: 44, stroke: "#000000", strokeWidth: 1, dash: "dotted")
        Text(x: 34, y: 66, label: "Mean age at selection = 34", anchor: "start", dx: 15, dy: 10, size: 14)
        Text(x: 44, y: 49, label: "Mean age at mission = 44", anchor: "start", dx: 15, dy: 10, size: 14)
        Text(
            x: 60,
            y: 20,
            label: "John Glenn was 77\\non his last mission -\\nthe oldest person to\\ntravel in space!",
            anchor: "start",
            dx: 6,
            size: 14,
        )
    }
}
`,
  },
  {
    id: "minard-campaign",
    title: "Minard",
    dataset: "minard",
    summary: "Path widths, labels, two CSVs",
    source: `Chart(
    data: "minard_troops.csv",
    title: "Napoleon's Russian Campaign",
    subtitle: "Inspired by the graphic of C.J. Minard",
    marginRight: 40
) {
    Theme(name: "void")
    Table cities = "minard_cities.csv"

    Scale(stroke: direction,
          range: ["A" => "burlywood", "R" => "black"],
          labels: ["A" => "Advance", "R" => "Retreat"],
          label: "Direction")
    Scale(strokeWidth: survivors, domain: [0, null], range: [0, 30],
          breaks: [50000, 100000, 200000, 300000, 340000],
          labels: ["50k", "100k", "200k", "300k", "340k"], label: "Troops")

    Space(long * lat) {
        Path(stroke: direction, strokeWidth: survivors, group: group)
    }

    Space(long * lat, data: cities) {
        Text(label: city, size: 6)
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
  dataFile: string;
  filesSignature: string;
  result: AlgrafRenderResult;
}

export function DemoPage(): React.ReactElement {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [selectedPresetId, setSelectedPresetId] = React.useState(DEFAULT_PRESET.id);
  const [source, setSource] = React.useState(DEFAULT_PRESET.source);
  const [dataText, setDataText] = React.useState("");
  const [auxiliaryDataTexts, setAuxiliaryDataTexts] = React.useState<Record<string, string>>({});
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
    loadAlgrafRuntime()
      .then((loaded) => {
        if (cancelled) return;
        setRuntime(loaded);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setRuntimeError(errorMessage(err));
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
    setAuxiliaryDataTexts({});

    fetchDatasetFiles(selectedDataset)
      .then(({ auxiliaryTexts, primaryText }) => {
        if (cancelled) return;
        setDataText(primaryText);
        setAuxiliaryDataTexts(auxiliaryTexts);
        setDataState("ready");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setAuxiliaryDataTexts({});
        setDataError(errorMessage(err));
        setDataState("error");
      });

    return () => {
      cancelled = true;
    };
  }, [dataRevision, selectedDataset]);

  const dataFiles = React.useMemo(
    () => ({
      [selectedDataset.file]: dataText,
      ...auxiliaryDataTexts,
    }),
    [auxiliaryDataTexts, dataText, selectedDataset.file],
  );
  const dataFilesSignature = React.useMemo(() => stableFilesSignature(dataFiles), [dataFiles]);

  const renderCurrent = React.useCallback(() => {
    if (!runtime || dataState !== "ready") {
      return;
    }

    const renderSource = source;
    const renderFiles = dataFiles;
    const renderFilesSignature = dataFilesSignature;
    setRendering(true);
    window.setTimeout(() => {
      try {
        setRenderSnapshot({
          source: renderSource,
          dataFile: selectedDataset.file,
          filesSignature: renderFilesSignature,
          result: runtime.render(renderSource, renderFiles),
        });
      } catch (err: unknown) {
        setRenderSnapshot({
          source: renderSource,
          dataFile: selectedDataset.file,
          filesSignature: renderFilesSignature,
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
  }, [dataFiles, dataFilesSignature, dataState, runtime, selectedDataset.file, source]);

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
      renderSnapshot.filesSignature === dataFilesSignature &&
      renderSnapshot.dataFile === selectedDataset.file,
  );
  const diagnostics = diagnosticsAreCurrent ? (result?.diagnostics ?? []) : [];
  const currentError = diagnosticsAreCurrent ? (result?.error ?? null) : null;
  const hasErrors = diagnostics.some((diagnostic) => diagnostic.severity === "error") || Boolean(currentError);
  const stats = React.useMemo(() => previewStats(result?.sidecar), [result?.sidecar]);
  const dataRows = React.useMemo(() => estimateRows(dataText, selectedDataset.file), [dataText, selectedDataset.file]);
  const auxiliaryFileCount = selectedDataset.auxiliaryFiles?.length ?? 0;
  const dataDetail = `${dataRows ?? selectedDataset.rows} rows, ${selectedDataset.columns} cols, ${formatBytes(dataText.length)}${linkedFilesDetail(auxiliaryFileCount)}`;

  return (
    <div className="playground-shell">
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
            files={dataFiles}
            onChange={setSource}
            runtime={runtime}
            value={source}
          />
        </div>

        <div className="pane data-pane">
          <PaneHeader
            icon={<Database size={17} aria-hidden="true" />}
            title={selectedDataset.file}
            detail={dataDetail}
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
    </div>
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

async function fetchDatasetFiles(dataset: DemoDataset): Promise<{
  auxiliaryTexts: Record<string, string>;
  primaryText: string;
}> {
  const auxiliaryFiles = dataset.auxiliaryFiles ?? [];
  const [primaryText, ...auxiliaryTexts] = await Promise.all([
    fetchDataFile(dataset),
    ...auxiliaryFiles.map((file) => fetchDataFile(file)),
  ]);

  return {
    primaryText,
    auxiliaryTexts: Object.fromEntries(
      auxiliaryFiles.map((file, index) => [file.file, auxiliaryTexts[index] ?? ""]),
    ),
  };
}

async function fetchDataFile(file: DemoDataFile): Promise<string> {
  const response = await fetch(file.url);
  if (!response.ok) {
    throw new Error(`failed to fetch ${file.url}: ${response.status}`);
  }
  return response.text();
}

function stableFilesSignature(files: Record<string, string>): string {
  const sortedEntries = Object.entries(files).sort(([left], [right]) => {
    if (left < right) return -1;
    if (left > right) return 1;
    return 0;
  });
  return JSON.stringify(Object.fromEntries(sortedEntries));
}

function linkedFilesDetail(count: number): string {
  if (count === 0) {
    return "";
  }
  return `, +${count} linked file${count === 1 ? "" : "s"}`;
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
