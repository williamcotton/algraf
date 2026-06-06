import React from "react";
import { AlertCircle, CheckCircle2, Code2, LoaderCircle, Play } from "lucide-react";
import { AlgrafEditor } from "algraf-editor";
import { type AlgrafDiagnostic, type AlgrafRenderResult } from "algraf-wasm";

import { AlgrafChart } from "../../AlgrafChart";
import { algrafEditorSetupOptions } from "../../editorSetup";
import { publicAssetUrl } from "../../publicAssets";
import { type RuntimeState, useAlgrafRuntime } from "./useAlgrafRuntime";

export interface LiveExampleProps {
  id: string;
  source: string;
  files: Record<string, string>;
  // Large bundled inputs (e.g. GeoJSON basemaps) are fetched from public assets
  // at render time instead of being inlined. Maps the filename referenced in the
  // source to its path under the demo's public directory.
  assets?: Record<string, string>;
}

export function LiveExample({ id, source, files, assets }: LiveExampleProps): React.ReactElement {
  const { runtime, state: runtimeState, error: runtimeError } = useAlgrafRuntime();
  const [value, setValue] = React.useState(source);
  const [result, setResult] = React.useState<AlgrafRenderResult | null>(null);
  const [rendering, setRendering] = React.useState(false);
  const [assetFiles, setAssetFiles] = React.useState<Record<string, string>>({});
  const [assetError, setAssetError] = React.useState<string | null>(null);

  const assetsKey = assets ? JSON.stringify(assets) : "";
  const assetsReady = !assets || Object.keys(assetFiles).length >= Object.keys(assets).length;

  React.useEffect(() => {
    if (!assets) {
      return;
    }
    let cancelled = false;
    setAssetError(null);
    Promise.all(
      Object.entries(assets).map(async ([name, path]) => {
        const response = await fetch(publicAssetUrl(path));
        if (!response.ok) {
          throw new Error(`failed to fetch ${path}: ${response.status}`);
        }
        return [name, await response.text()] as const;
      }),
    )
      .then((entries) => {
        if (cancelled) return;
        setAssetFiles(Object.fromEntries(entries));
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setAssetError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
    // assetsKey captures the contents of `assets`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [assetsKey]);

  const renderFiles = React.useMemo(() => ({ ...files, ...assetFiles }), [files, assetFiles]);

  const renderCurrent = React.useCallback(() => {
    if (!runtime || !assetsReady) {
      return;
    }
    const renderSource = value;
    setRendering(true);
    window.setTimeout(() => {
      try {
        setResult(runtime.render(renderSource, renderFiles));
      } catch (err: unknown) {
        setResult({ svg: null, sidecar: null, diagnostics: [], error: errorMessage(err) });
      } finally {
        setRendering(false);
      }
    }, 0);
  }, [assetsReady, renderFiles, runtime, value]);

  React.useEffect(() => {
    if (runtimeState !== "ready" || !assetsReady) {
      return;
    }
    const timer = window.setTimeout(renderCurrent, 200);
    return () => window.clearTimeout(timer);
  }, [assetsReady, renderCurrent, runtimeState]);

  const diagnostics = result?.diagnostics ?? [];
  const errorCount = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const warningCount = diagnostics.length - errorCount;
  const previewMessage =
    result?.error ?? runtimeError ?? assetError ?? (assetsReady ? "Loading the browser runtime" : "Loading map data");
  const hasError = errorCount > 0 || Boolean(result?.error);

  return (
    <div className="tutorial-live-pair">
      <section className="tutorial-example-panel tutorial-example-editor-panel">
        <div className="mini-panel-header">
          <span>
            <Code2 size={16} aria-hidden="true" />
            {id}.ag
          </span>
          <button className="compact-button" type="button" disabled={!runtime} onClick={renderCurrent}>
            {rendering ? <LoaderCircle className="spin" size={15} aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
            Render
          </button>
        </div>
        <div className="tutorial-example-editor">
          <AlgrafEditor
            diagnostics={diagnosticsForEditor(diagnostics, result?.error ?? null)}
            files={renderFiles}
            modelUri={`inmemory://algraf/docs/${id}.ag`}
            onChange={setValue}
            runtime={runtime}
            setupOptions={algrafEditorSetupOptions}
            value={value}
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
  );
}

function DocsStatus({ state }: { state: RuntimeState }): React.ReactElement {
  const label = state === "ready" ? "ready" : state === "loading" ? "loading" : "error";
  return <span className={`mini-status mini-status-${state}`}>WASM {label}</span>;
}

function diagnosticsForEditor(diagnostics: AlgrafDiagnostic[], error: string | null): AlgrafDiagnostic[] {
  if (!error) {
    return diagnostics;
  }
  return [
    ...diagnostics,
    { code: "Runtime", severity: "error", message: error, span: { start: 0, end: 0 } },
  ];
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
