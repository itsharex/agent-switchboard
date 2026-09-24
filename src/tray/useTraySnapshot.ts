import { useMessageState } from "../i18n/use-message-state";
import { useCallback, useEffect, useRef, useState } from "react";
import { getTraySnapshot, onTrayChanged, type TraySnapshot } from "../api/client";
import { applyLanguagePreference } from "../i18n/current";

export function useTraySnapshot() {
  const [snapshot, setSnapshot] = useState<TraySnapshot | null>(null);
  const [error, setError] = useMessageState();
  const [initialized, setInitialized] = useState(false);
  const refreshRef = useRef<() => Promise<void>>(async () => {});
  const refresh = useCallback(() => refreshRef.current(), []);

  useEffect(() => {
    let disposed = false;
    let revision = 0;
    let unlisten: (() => void) | undefined;
    const reload = async () => {
      const request = ++revision;
      try {
        const next = await getTraySnapshot();
        if (!disposed && request === revision) {
          if (next.settings) applyLanguagePreference(next.settings.language);
          setSnapshot(next);
          setError(null);
        }
      } catch (caught) {
        if (!disposed && request === revision) setError(caught);
      }
    };
    refreshRef.current = reload;
    void (async () => {
      try {
        const stop = await onTrayChanged(() => { void reload(); });
        if (disposed) { stop(); return; }
        unlisten = stop;
        await reload();
      } catch (caught) {
        if (!disposed) setError(caught);
      } finally {
        if (!disposed) setInitialized(true);
      }
    })();
    return () => { disposed = true; revision++; unlisten?.(); refreshRef.current = async () => {}; };
  }, []);
  return { snapshot, error, initialized, refresh };
}
