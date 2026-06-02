import React from "react";
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  Code2,
  ExternalLink,
  LoaderCircle,
  Play,
  Terminal,
} from "lucide-react";

import { AlgrafChart } from "../AlgrafChart";
import { AlgrafEditor } from "../AlgrafEditor";
import {
  type AlgrafDiagnostic,
  type AlgrafRenderResult,
  type AlgrafRuntime,
  loadAlgrafRuntime,
} from "../algrafWasm";

type LoadState = "loading" | "ready" | "error";

interface RoutedPageProps {
  navigate: (path: string, event?: React.MouseEvent<HTMLAnchorElement>) => void;
  routeHref: (path: string) => string;
}

interface TutorialStep {
  id: string;
  title: string;
  lesson: string;
  source: string;
}

const TUTORIAL_DATA = `species,flipper_length_mm,body_mass_g,island
Adelie,181,3750,Torgersen
Adelie,186,3800,Torgersen
Adelie,195,3350,Dream
Adelie,193,3450,Dream
Chinstrap,196,3550,Dream
Chinstrap,201,3950,Dream
Chinstrap,207,4050,Dream
Chinstrap,210,4100,Dream
Gentoo,210,4400,Biscoe
Gentoo,215,4850,Biscoe
Gentoo,222,5250,Biscoe
Gentoo,230,5550,Biscoe
`;

const TUTORIAL_STEPS: TutorialStep[] = [
  {
    id: "chart-space-point",
    title: "Start with data, space, and one mark",
    lesson:
      "`Chart(data: ...)` names the table. `Space(flipper_length_mm * body_mass_g)` chooses x and y. `Point()` draws one mark per row.",
    source: `Chart(data: "penguins_lite.csv", width: 640, height: 380, title: "Penguin body mass") {
    Space(flipper_length_mm * body_mass_g) {
        Point()
    }
}
`,
  },
  {
    id: "theme-guides",
    title: "Name the axes",
    lesson:
      "`Theme(name: \"minimal\")` selects a presentation preset. `Guide(axis: ..., label: ...)` gives the generated axes readable labels.",
    source: `Chart(data: "penguins_lite.csv", width: 640, height: 380, title: "Penguin body mass") {
    Theme(name: "minimal")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(size: 4)
    }
}
`,
  },
  {
    id: "mapped-aesthetics",
    title: "Map data to visual properties",
    lesson:
      "`fill: species` is a mapping, not a literal color. Algraf trains a categorical scale, colors points, and emits a legend.",
    source: `Chart(data: "penguins_lite.csv", width: 640, height: 380, title: "Penguin body mass") {
    Theme(name: "minimal")
    Scale(fill: species, palette: "accent")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: species, size: 4.8, alpha: 0.78)
    }
}
`,
  },
  {
    id: "layers-interaction",
    title: "Add a layer and interaction metadata",
    lesson:
      "Layers share the inherited space. `Smooth(method: \"lm\")` adds a fitted line, while `tooltip` and `highlight` feed the browser viewer sidecar.",
    source: `Chart(data: "penguins_lite.csv", width: 640, height: 380, title: "Penguin body mass") {
    Theme(name: "minimal")
    Scale(fill: species, palette: "accent")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(
            fill: species,
            size: 4.8,
            alpha: 0.78,
            tooltip: [species, island, flipper_length_mm, body_mass_g],
            highlight: species
        )
        Smooth(method: "lm", stroke: "#263238", strokeWidth: 2.2)
    }
}
`,
  },
];

const HOMEBREW_COMMANDS = `brew tap williamcotton/algraf
brew install algraf
`;

const CLI_COMMANDS = `algraf check examples/scatter.ag
algraf render examples/scatter.ag --output scatter.svg
algraf schema examples/scatter.ag --json
`;

const BROWSER_COMMANDS = `cd demo
npm install
npm run dev
`;

export function DocsPage({ navigate, routeHref }: RoutedPageProps): React.ReactElement {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [runtimeState, setRuntimeState] = React.useState<LoadState>("loading");
  const [runtimeError, setRuntimeError] = React.useState<string | null>(null);
  const files = React.useMemo(() => ({ "penguins_lite.csv": TUTORIAL_DATA }), []);

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

  return (
    <div className="docs-page">
      <section className="docs-hero">
        <p className="eyebrow">Guided quickstart</p>
        <h1>Build a chart one idea at a time.</h1>
        <p>
          Each section below has its own live editor and rendered preview. Edit the source in place, watch diagnostics
          update, and compare the result without switching to a separate workspace.
        </p>
        <div className="hero-actions">
          <a className="primary-link" href={routeHref("/demos")} onClick={(event) => navigate("/demos", event)}>
            <Code2 size={16} aria-hidden="true" />
            Open demos
          </a>
          <a className="secondary-link" href="https://github.com/williamcotton/algraf/blob/main/docs/ALGRAF_SPEC.md">
            <ExternalLink size={16} aria-hidden="true" />
            Full spec
          </a>
        </div>
      </section>

      <section className="tutorial-sections" aria-label="Algraf guided tutorial">
        {TUTORIAL_STEPS.map((step, index) => (
          <TutorialExample
            files={files}
            index={index}
            key={step.id}
            runtime={runtime}
            runtimeError={runtimeError}
            runtimeState={runtimeState}
            step={step}
          />
        ))}
      </section>

      <section className="docs-grid">
        <article className="docs-panel">
          <h2>
            <Terminal size={18} aria-hidden="true" />
            Install
          </h2>
          <p>Install the packaged binary with Homebrew, then use `algraf` directly.</p>
          <pre>
            <code>{HOMEBREW_COMMANDS}</code>
          </pre>
        </article>

        <article className="docs-panel">
          <h2>
            <Terminal size={18} aria-hidden="true" />
            Run
          </h2>
          <p>Validate source, render SVG, and inspect inferred schemas from the command line.</p>
          <pre>
            <code>{CLI_COMMANDS}</code>
          </pre>
        </article>

        <article className="docs-panel">
          <h2>
            <Code2 size={18} aria-hidden="true" />
            Browser
          </h2>
          <p>The demo builds `algraf-wasm`, serves bundled datasets, and calls the same render pipeline in memory.</p>
          <pre>
            <code>{BROWSER_COMMANDS}</code>
          </pre>
        </article>

        <article className="docs-panel">
          <h2>What to learn next</h2>
          <p>
            Open `/demos` for larger presets covering trend lines, temporal parsing, histograms, ribbons,
            multiple data files, annotations, and interactive metadata.
          </p>
          <a className="docs-inline-link" href={routeHref("/demos")} onClick={(event) => navigate("/demos", event)}>
            Go to demos
            <ArrowRight size={15} aria-hidden="true" />
          </a>
        </article>
      </section>
    </div>
  );
}

function TutorialExample({
  files,
  index,
  runtime,
  runtimeError,
  runtimeState,
  step,
}: {
  files: Record<string, string>;
  index: number;
  runtime: AlgrafRuntime | null;
  runtimeError: string | null;
  runtimeState: LoadState;
  step: TutorialStep;
}): React.ReactElement {
  const [source, setSource] = React.useState(step.source);
  const [result, setResult] = React.useState<AlgrafRenderResult | null>(null);
  const [rendering, setRendering] = React.useState(false);

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
  const hasError = errorCount > 0 || Boolean(result?.error);

  return (
    <article className="tutorial-section">
      <div className="tutorial-section-copy">
        <p className="eyebrow">Step {index + 1}</p>
        <h2>{step.title}</h2>
        <p>{step.lesson}</p>
      </div>

      <div className="tutorial-live-pair">
        <section className="tutorial-example-panel tutorial-example-editor-panel">
          <div className="mini-panel-header">
            <span>
              <Code2 size={16} aria-hidden="true" />
              {step.id}.ag
            </span>
            <button className="compact-button" type="button" disabled={!runtime} onClick={renderCurrent}>
              {rendering ? <LoaderCircle className="spin" size={15} aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
              Render
            </button>
          </div>
          <div className="tutorial-example-editor">
            <AlgrafEditor
              diagnostics={diagnosticsForEditor(diagnostics, result?.error ?? null)}
              files={files}
              modelUri={`inmemory://algraf/docs/${step.id}.ag`}
              onChange={setSource}
              runtime={runtime}
              value={source}
            />
          </div>
        </section>

        <section className="tutorial-example-panel tutorial-example-preview-panel">
          <div className="mini-panel-header">
            <span>
              {rendering ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}
              Rendered SVG
            </span>
            <DocsStatus state={runtimeState} />
          </div>
          <div className="tutorial-example-preview-stage">
            {result?.svg ? (
              <AlgrafChart sidecar={result.sidecar} svg={result.svg} />
            ) : (
              <div className="mini-empty-preview">{previewMessage}</div>
            )}
          </div>
          <div className={`tutorial-diagnostics ${hasError ? "tutorial-diagnostics-error" : ""}`}>
            {result?.error ? (
              <>
                <AlertCircle size={15} aria-hidden="true" />
                {result.error}
              </>
            ) : (
              <>
                <CheckCircle2 size={15} aria-hidden="true" />
                {errorCount} errors, {warningCount} warnings
              </>
            )}
          </div>
        </section>
      </div>
    </article>
  );
}

function DocsStatus({ state }: { state: LoadState }): React.ReactElement {
  const label = state === "ready" ? "ready" : state === "loading" ? "loading" : "error";
  return <span className={`mini-status mini-status-${state}`}>WASM {label}</span>;
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

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
