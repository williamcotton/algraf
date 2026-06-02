import React from "react";

import { type AlgrafRuntime, loadAlgrafRuntime } from "../../algrafWasm";

export type RuntimeState = "loading" | "ready" | "error";

export interface RuntimeHandle {
  runtime: AlgrafRuntime | null;
  state: RuntimeState;
  error: string | null;
}

// The docs section renders many live editors at once. Instantiating the WASM
// runtime per editor would refetch and recompile the module each time, so the
// loader promise is cached at module scope and shared across every hook caller.
let runtimePromise: Promise<AlgrafRuntime> | null = null;

function sharedRuntime(): Promise<AlgrafRuntime> {
  if (!runtimePromise) {
    runtimePromise = loadAlgrafRuntime().catch((err) => {
      // Allow a later caller to retry after a transient failure.
      runtimePromise = null;
      throw err;
    });
  }
  return runtimePromise;
}

export function useAlgrafRuntime(): RuntimeHandle {
  const [runtime, setRuntime] = React.useState<AlgrafRuntime | null>(null);
  const [state, setState] = React.useState<RuntimeState>("loading");
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    setState("loading");
    sharedRuntime()
      .then((loaded) => {
        if (cancelled) return;
        setRuntime(loaded);
        setState("ready");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
        setState("error");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return { runtime, state, error };
}
