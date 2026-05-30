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

const DATA_URL = "/data/penguins.json";
const DATA_FILE = "penguins.json";

const DEFAULT_SOURCE = `Chart(data: "penguins.json", width: 760, height: 500) {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point(
            fill: species,
            alpha: 0.82,
            size: 4,
            tooltip: [species, flipper_length, body_mass],
            highlight: species
        )
    }
}
`;

type LoadState = "loading" | "ready" | "error";

interface PreviewStats {
  marks: number;
  groups: number;
}

interface RenderSnapshot {
  source: string;
  dataText: string;
  result: AlgrafRenderResult;
}

export function App(): React.ReactElement {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [runtimeState, setRuntimeState] = React.useState<LoadState>("loading");
  const [source, setSource] = React.useState(DEFAULT_SOURCE);
  const [dataText, setDataText] = React.useState("");
  const [dataState, setDataState] = React.useState<LoadState>("loading");
  const [dataRevision, setDataRevision] = React.useState(0);
  const [renderSnapshot, setRenderSnapshot] = React.useState<RenderSnapshot | null>(null);
  const [rendering, setRendering] = React.useState(false);
  const [runtimeError, setRuntimeError] = React.useState<string | null>(null);
  const [dataError, setDataError] = React.useState<string | null>(null);

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

    fetch(DATA_URL)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`failed to fetch ${DATA_URL}: ${response.status}`);
        }
        return response.json() as Promise<unknown>;
      })
      .then((json) => {
        if (cancelled) return;
        setDataText(JSON.stringify(json, null, 2));
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
  }, [dataRevision]);

  const renderCurrent = React.useCallback(() => {
    if (!runtime) {
      return;
    }

    const renderSource = source;
    const renderDataText = dataText;
    setRendering(true);
    window.setTimeout(() => {
      try {
        JSON.parse(renderDataText);
        setRenderSnapshot({
          source: renderSource,
          dataText: renderDataText,
          result: runtime.render(renderSource, { [DATA_FILE]: renderDataText }),
        });
      } catch (err: unknown) {
        setRenderSnapshot({
          source: renderSource,
          dataText: renderDataText,
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
  }, [dataText, runtime, source]);

  React.useEffect(() => {
    if (!runtime || dataState !== "ready") {
      return;
    }

    const timer = window.setTimeout(() => {
      setRendering(true);
      renderCurrent();
    }, 260);

    return () => window.clearTimeout(timer);
  }, [dataState, renderCurrent, runtime]);

  const result = renderSnapshot?.result ?? null;
  const diagnosticsAreCurrent = Boolean(renderSnapshot && renderSnapshot.source === source && renderSnapshot.dataText === dataText);
  const diagnostics = diagnosticsAreCurrent ? (result?.diagnostics ?? []) : [];
  const currentError = diagnosticsAreCurrent ? (result?.error ?? null) : null;
  const hasErrors = diagnostics.some((diagnostic) => diagnostic.severity === "error") || Boolean(currentError);
  const stats = React.useMemo(() => previewStats(result?.sidecar), [result?.sidecar]);

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <h1>Algraf WASM Playground</h1>
          <p>{DATA_FILE} fetched from the dev server and rendered in the browser runtime.</p>
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

      <section className="workspace-grid">
        <div className="pane editor-pane">
          <PaneHeader icon={<Code2 size={17} aria-hidden="true" />} title="Algraf" detail={`${source.length} bytes`} />
          <AlgrafEditor diagnostics={diagnostics} onChange={setSource} value={source} />
        </div>

        <div className="pane data-pane">
          <PaneHeader
            icon={<Database size={17} aria-hidden="true" />}
            title="Network JSON"
            detail={DATA_URL}
            action={
              <button className="compact-button" type="button" onClick={() => setDataRevision((revision) => revision + 1)}>
                <RefreshCw size={15} aria-hidden="true" />
                Reload
              </button>
            }
          />
          <textarea
            aria-label="JSON data"
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

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
