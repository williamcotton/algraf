import React from "react";
import { ArrowRight, CheckCircle2, Code2, LoaderCircle, Play, Terminal } from "lucide-react";
import { AlgrafEditor } from "algraf-editor";
import {
  type AlgrafDiagnostic,
  type AlgrafRenderResult,
  type AlgrafRuntime,
  loadAlgrafRuntime,
} from "algraf-wasm";

import { AlgrafChart } from "../AlgrafChart";
import { algrafEditorSetupOptions } from "../editorSetup";
import { publicAssetUrl } from "../publicAssets";

type LoadState = "loading" | "ready" | "error";

interface RoutedPageProps {
  navigate: (path: string, event?: React.MouseEvent<HTMLAnchorElement>) => void;
  routeHref: (path: string) => string;
}

const HOMEPAGE_DATA_PATH = "homepage-starter.csv";

const STARTER_DATA = `week,value,series
1,19,Forecast
2,24,Forecast
3,28,Forecast
4,31,Forecast
5,35,Forecast
6,39,Forecast
1,16,Actual
2,20,Actual
3,26,Actual
4,29,Actual
5,33,Actual
6,36,Actual
`;

const STARTER_SOURCE = `Chart(data: "homepage-starter.csv", width: 620, height: 360, title: "A small Algraf chart") {
    Theme(name: "minimal")
    Scale(fill: series, palette: "accent")
    Guide(axis: x, label: "Week")
    Guide(axis: y, label: "Value")

    Space(week * value) {
        Line(stroke: series, strokeWidth: 2.4)
        Point(
            fill: series,
            size: 4.8,
            tooltip: [series, week, value],
            highlight: series
        )
    }
}
`;

const HOMEBREW_COMMANDS = `brew tap williamcotton/algraf
brew install algraf
`;

const EXPORT_COMMANDS = `algraf render demo/public/homepage.ag --output /tmp/algraf-homepage.svg
algraf render demo/public/homepage.ag --output /tmp/algraf-homepage.png
`;

export function HomePage({ navigate, routeHref }: RoutedPageProps): React.ReactElement {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [runtimeState, setRuntimeState] = React.useState<LoadState>("loading");
  const [source, setSource] = React.useState(STARTER_SOURCE);
  const [result, setResult] = React.useState<AlgrafRenderResult | null>(null);
  const [rendering, setRendering] = React.useState(false);
  const [runtimeError, setRuntimeError] = React.useState<string | null>(null);
  const files = React.useMemo(() => ({ [HOMEPAGE_DATA_PATH]: STARTER_DATA }), []);

  React.useEffect(() => {
    let cancelled = false;
    setRuntimeState("loading");
    loadAlgrafRuntime({ wasmUrl: publicAssetUrl("wasm/algraf.wasm") })
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

  const renderCurrent = React.useCallback(() => {
    if (!runtime) {
      return;
    }
    const renderSource = source;
    setRendering(true);
    window.setTimeout(() => {
      try {
        setResult(runtime.render(renderSource, files));
      } catch (err: unknown) {
        setResult({
          svg: null,
          sidecar: null,
          diagnostics: [],
          error: errorMessage(err),
        });
      } finally {
        setRendering(false);
      }
    }, 0);
  }, [files, runtime, source]);

  React.useEffect(() => {
    if (runtimeState !== "ready") {
      return;
    }
    const timer = window.setTimeout(renderCurrent, 220);
    return () => window.clearTimeout(timer);
  }, [renderCurrent, runtimeState]);

  const diagnostics = result?.diagnostics ?? [];
  const errorCount = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const warningCount = diagnostics.length - errorCount;
  const previewMessage = result?.error ?? runtimeError ?? "Loading the browser runtime";

  return (
    <div className="home-page">
      <section className="hero-section">
        <div className="hero-intro">
          <p className="eyebrow">A chart language with the toolchain included</p>
          <h1>Algraf</h1>
          <p>
            A block-scoped grammar-of-graphics DSL that parses, validates, serves editor intelligence,
            and renders deterministic SVG from one Rust binary.
          </p>
          <div className="hero-actions">
            <a className="primary-link" href={routeHref("/docs")} onClick={(event) => navigate("/docs", event)}>
              <ArrowRight size={16} aria-hidden="true" />
              Read the quickstart
            </a>
            <a className="secondary-link" href={routeHref("/demos")} onClick={(event) => navigate("/demos", event)}>
              <Code2 size={16} aria-hidden="true" />
              Open demos
            </a>
          </div>
        </div>

        <div className="hero-demo-tool">
          <div className="mini-editor-panel">
            <div className="mini-panel-header">
              <span>
                <Code2 size={16} aria-hidden="true" />
                starter.ag
              </span>
              <button className="compact-button" type="button" disabled={!runtime} onClick={renderCurrent}>
                {rendering ? <LoaderCircle className="spin" size={15} aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
                Render
              </button>
            </div>
            <div className="mini-editor-host">
              <AlgrafEditor
                diagnostics={diagnosticsForEditor(diagnostics, result?.error ?? null)}
                files={files}
                onChange={setSource}
                runtime={runtime}
                setupOptions={algrafEditorSetupOptions}
                value={source}
              />
            </div>
          </div>

          <div className="mini-preview-panel">
            <div className="mini-panel-header">
              <span>
                {rendering ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}
                Browser render
              </span>
              <MiniStatus state={runtimeState} />
            </div>
            <div className="mini-preview-stage">
              {result?.svg ? (
                <AlgrafChart sidecar={result.sidecar} svg={result.svg} />
              ) : (
                <div className="mini-empty-preview">{previewMessage}</div>
              )}
            </div>
            <div className="mini-diagnostics">
              {result?.error ? result.error : `${errorCount} errors, ${warningCount} warnings`}
            </div>
          </div>
        </div>
      </section>

      <section className="install-strip" aria-label="Install Algraf">
        <div>
          <p className="eyebrow">Install</p>
          <h2>Get the packaged binary with Homebrew.</h2>
          <p>Then run `algraf check`, `algraf render`, `algraf schema`, or `algraf lsp` directly from your shell.</p>
        </div>
        <pre>
          <code>{HOMEBREW_COMMANDS}</code>
        </pre>
      </section>

      <section className="install-strip" aria-label="Render the homepage chart">
        <div>
          <p className="eyebrow">Render locally</p>
          <h2>Export this chart to SVG or PNG.</h2>
          <p>The homepage source is checked in at `demo/public/homepage.ag` with data in `demo/public/data/`.</p>
        </div>
        <pre>
          <code>{EXPORT_COMMANDS}</code>
        </pre>
      </section>

      <section className="language-highlights" aria-label="Algraf capabilities">
        <article className="feature-card">
          <h2>Language first</h2>
          <p>Charts are source files with scoped blocks, algebraic spaces, typed data validation, and stable diagnostics.</p>
        </article>
        <article className="feature-card">
          <h2>One runtime</h2>
          <p>The CLI, LSP, WASM runtime, and renderer share the same parser, analyzer, registry, and render pipeline.</p>
        </article>
        <article className="feature-card">
          <h2>Deterministic output</h2>
          <p>Algraf trains scales and emits SVG predictably, so examples, tests, docs, and embedded previews stay comparable.</p>
        </article>
      </section>

      <section className="cli-section" aria-label="Native CLI">
        <header className="cli-section-head">
          <p className="eyebrow">
            <Terminal size={15} aria-hidden="true" />
            On the command line
          </p>
          <h2>Render targets, data in, scriptable.</h2>
          <p>
            The <code>algraf</code> binary renders the same source files into SVG, PNG, JSON draw-lists,
            and interactive SVG &mdash; reading CSV, Parquet, Arrow streams, SQLite, or piped stdin from
            tools like PDL.
          </p>
        </header>

        <div className="cli-subgroup">
          <p className="cli-subgroup-label">Render targets</p>
          <div className="cli-chip-row">
            <span className="cli-chip"><code>--output chart.svg</code><small>deterministic SVG</small></span>
            <span className="cli-chip"><code>--output chart.png</code><small>resvg-rasterized PNG</small></span>
            <span className="cli-chip"><code>--format svg+json</code><small>SVG + pickable-marks sidecar</small></span>
            <span className="cli-chip"><code>--format draw-list</code><small>JSON primitives</small></span>
            <span className="cli-chip"><code>--interactive</code><small>tooltip + highlight runtime</small></span>
          </div>
        </div>

        <div className="cli-subgroup">
          <p className="cli-subgroup-label">Subcommands</p>
          <div className="cli-chip-row">
            <span className="cli-chip"><code>algraf render</code><small>SVG/PNG/JSON</small></span>
            <span className="cli-chip"><code>algraf check</code><small>parse + analyze</small></span>
            <span className="cli-chip"><code>algraf format</code><small>canonical format</small></span>
            <span className="cli-chip"><code>algraf schema</code><small>resolved schema</small></span>
            <span className="cli-chip"><code>algraf ast</code><small>parse tree</small></span>
            <span className="cli-chip"><code>algraf ir</code><small>semantic IR</small></span>
            <span className="cli-chip"><code>algraf lsp</code><small>language server</small></span>
          </div>
        </div>

        <div className="cli-subgroup">
          <p className="cli-subgroup-label">Data formats accepted</p>
          <div className="cli-chip-row">
            <span className="cli-chip"><code>csv</code></span>
            <span className="cli-chip"><code>tsv</code></span>
            <span className="cli-chip"><code>json</code></span>
            <span className="cli-chip"><code>ndjson</code></span>
            <span className="cli-chip"><code>geojson</code></span>
            <span className="cli-chip"><code>parquet</code></span>
            <span className="cli-chip"><code>arrow-stream</code></span>
            <span className="cli-chip"><code>sqlite</code></span>
          </div>
        </div>

        <div className="cli-subgroup">
          <p className="cli-subgroup-label">
            Compose with{" "}
            <a className="cli-inline-link" href="https://williamcotton.github.io/pdl/">PDL</a>
          </p>
          <pre className="cli-snippet"><code>{`pdl run prep.pdl --stdout-format arrow-stream \\
  | algraf render chart.ag --data - --data-format arrow-stream \\
  --output chart.svg`}</code></pre>
        </div>
      </section>

      <section className="home-band">
        <div>
          <h2>Move from source to SVG without switching tools</h2>
          <p>
            Use the docs page for a guided first chart, then open the demos for larger examples, bundled data,
            diagnostics, and interactive sidecar previews.
          </p>
        </div>
        <a className="primary-link" href={routeHref("/demos")} onClick={(event) => navigate("/demos", event)}>
          Explore demos
          <ArrowRight size={16} aria-hidden="true" />
        </a>
      </section>
    </div>
  );
}

function diagnosticsForEditor(diagnostics: AlgrafDiagnostic[], error: string | null): AlgrafDiagnostic[] {
  if (!error) {
    return diagnostics;
  }
  return [
    ...diagnostics,
    {
      code: "Runtime",
      severity: "error",
      message: error,
      span: { start: 0, end: 0 },
    },
  ];
}

function MiniStatus({ state }: { state: LoadState }): React.ReactElement {
  const label = state === "ready" ? "ready" : state === "loading" ? "loading" : "error";
  return <span className={`mini-status mini-status-${state}`}>WASM {label}</span>;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
