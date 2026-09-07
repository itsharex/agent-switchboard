import { useCallback, useEffect, useState } from "react";
import type { CommandError, ExtensionsWorkspace } from "../api/client";
import { listExtensions } from "../api/client";
import { useExtensionImport } from "./extensions/useExtensionImport";
import { useExtensionLibrary } from "./extensions/useExtensionLibrary";
import { useSkillLibrary } from "./extensions/useSkillLibrary";
import type { ExtensionsDeps } from "./extensions/extension-ops";

/** Owns the extension-library workspace state and serialises every library
 * or plan mutation into the shared operation frame (busy / error gate). */
export function useExtensions({ busy, setBusy, clearError, onError }: ExtensionsDeps) {
  const [workspace, setWorkspace] = useState<ExtensionsWorkspace | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    void listExtensions()
      .then((next) => {
        if (current) setWorkspace(next);
      })
      .catch((caught) => {
        if (current) onError(caught as CommandError);
      })
      .finally(() => {
        if (current) setLoaded(true);
      });
    return () => {
      current = false;
    };
  }, [onError]);

  const refresh = useCallback(async (): Promise<ExtensionsWorkspace | null> => {
    try {
      const next = await listExtensions();
      setWorkspace(next);
      return next;
    } catch (caught) {
      onError(caught as CommandError);
      return null;
    }
  }, [onError]);

  const runExclusive = useCallback(
    async <T>(action: () => Promise<T>): Promise<T | null> => {
      if (busy) return null;
      setBusy(true);
      clearError();
      try {
        return await action();
      } catch (caught) {
        onError(caught as CommandError);
        return null;
      } finally {
        setBusy(false);
      }
    },
    [busy, clearError, onError, setBusy],
  );

  const library = useExtensionLibrary({ refresh, runExclusive });
  const importing = useExtensionImport({ refresh, runExclusive });
  const skills = useSkillLibrary({ refresh, runExclusive });

  return {
    workspace,
    loaded,
    refresh,
    ...library,
    ...importing,
    ...skills,
  };
}
