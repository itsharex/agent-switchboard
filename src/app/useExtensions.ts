import { useCallback, useEffect, useRef, useState } from "react";
import type { CommandError, ExtensionsWorkspace } from "../api/client";
import { listExtensions } from "../api/client";
import { useExtensionImport } from "./extensions/useExtensionImport";
import { useExtensionLibrary } from "./extensions/useExtensionLibrary";
import { useSkillLibrary } from "./extensions/useSkillLibrary";
import type { ExtensionsDeps } from "./extensions/extension-ops";

function useExtensionOperationFrame(deps: ExtensionsDeps) {
  const latest = useRef(deps);
  latest.current = deps;
  const busyRef = useRef(deps.busy);
  busyRef.current = deps.busy;
  const operationInFlight = useRef(false);
  const [working, setWorking] = useState(false);
  const isBusy = useCallback(() => busyRef.current || operationInFlight.current, []);
  const runRead = useCallback(async <T>(action: () => Promise<T>): Promise<T | null> => {
    try {
      return await action();
    } catch (caught) {
      latest.current.onError(caught as CommandError);
      return null;
    }
  }, []);
  const runExclusive = useCallback(async <T>(action: () => Promise<T>): Promise<T | null> => {
    if (isBusy()) return null;
    operationInFlight.current = true;
    busyRef.current = true;
    setWorking(true);
    latest.current.setBusy(true);
    latest.current.clearError();
    try {
      return await runRead(action);
    } finally {
      operationInFlight.current = false;
      // Release synchronously: the next awaited step may precede React's render.
      busyRef.current = false;
      setWorking(false);
      latest.current.setBusy(false);
    }
  }, [isBusy, runRead]);
  return { busy: deps.busy || working, isBusy, runRead, runExclusive };
}

/** Owns the extension-library workspace state and serialises every library
 * or plan mutation into the shared operation frame (busy / error gate). */
export function useExtensions(deps: ExtensionsDeps) {
  const [workspace, setWorkspace] = useState<ExtensionsWorkspace | null>(null);
  const [loaded, setLoaded] = useState(false);
  const frame = useExtensionOperationFrame(deps);
  const { runRead, runExclusive } = frame;

  useEffect(() => {
    let current = true;
    void runRead(listExtensions)
      .then((next) => {
        if (current && next) setWorkspace(next);
      })
      .finally(() => {
        if (current) setLoaded(true);
      });
    return () => {
      current = false;
    };
  }, [runRead]);

  const refresh = useCallback(async (): Promise<ExtensionsWorkspace | null> => {
    const next = await runRead(listExtensions);
    if (next) setWorkspace(next);
    return next;
  }, [runRead]);

  const library = useExtensionLibrary({ refresh, runExclusive, runRead });
  const importing = useExtensionImport({ refresh, runExclusive });
  const skills = useSkillLibrary({ refresh, runExclusive, runRead });

  return {
    workspace,
    loaded,
    refresh,
    ...frame,
    ...library,
    ...importing,
    ...skills,
  };
}
