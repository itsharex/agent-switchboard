import { useMessageState } from "../i18n/use-message-state";
import { useCallback, useEffect, useRef, useState } from "react";
import { onTrayChanged } from "../api/client";

export interface CachedQuery<T> {
  data: T | null;
  querying: boolean;
  error: string | null;
  run: () => Promise<void>;
}

/** Rows and the tray consume the backend cache. Only the refresh action
 * contacts upstream; renderer lifetime never controls the refresh cadence. */
export function useCachedQuery<T>(
  key: string,
  revision: string,
  read: (key: string) => Promise<T | null>,
  query: (key: string) => Promise<unknown>,
): CachedQuery<T> {
  const [data, setData] = useState<T | null>(null);
  const [querying, setQuerying] = useState(false);
  const [error, setError] = useMessageState();
  const runRef = useRef<() => Promise<void>>(async () => {});
  const run = useCallback(() => runRef.current(), []);

  useEffect(() => {
    let disposed = false;
    let readVersion = 0;
    let requestVersion = 0;
    let cachedValue = "null";
    let unlisten: (() => void) | undefined;
    setData(null);
    setError(null);
    setQuerying(false);
    const reload = async () => {
      const version = ++readVersion;
      try {
        const next = await read(key);
        if (!disposed && version === readVersion) {
          const value = JSON.stringify(next) ?? "null";
          if (value !== cachedValue) setError(null);
          cachedValue = value;
          setData(next);
        }
      } catch (caught) {
        if (!disposed && version === readVersion) setError(caught);
      }
    };
    runRef.current = async () => {
      const version = ++requestVersion;
      setQuerying(true);
      setError(null);
      let failure: unknown = null;
      try { await query(key); }
      catch (caught) { failure = caught; }
      if (disposed) return;
      // Network completion re-reads the cache instead of cancelling event
      // reads or displaying a second, independently ordered result.
      await reload();
      if (!disposed && version === requestVersion) {
        if (failure) setError(failure);
        setQuerying(false);
      }
    };
    void (async () => {
      try {
        const stop = await onTrayChanged(() => { void reload(); });
        if (disposed) { stop(); return; }
        unlisten = stop;
      } catch (caught) {
        if (!disposed) setError(caught);
      }
      if (!disposed) await reload();
    })();
    return () => {
      disposed = true;
      unlisten?.();
      runRef.current = async () => {};
    };
  }, [key, revision, read, query]);

  return { data, querying, error, run };
}
