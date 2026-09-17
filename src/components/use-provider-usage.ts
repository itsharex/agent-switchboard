import { useCallback, useEffect, useRef, useState } from "react";
import {
  onTrayChanged,
  queryProfileUsage,
  readProfileUsage,
  type ProviderProfile,
  type UsageSummary,
} from "../api/client";

/** The card owns queries so collapsing its details does not stop them. The
 * backend scheduler owns automatic re-query timing; this hook only performs
 * the mount-time first read and manual refreshes, and adopts newer cache
 * entries announced through tray-changed. */
export function useProviderUsage(profile: ProviderProfile) {
  const revision = JSON.stringify(profile.usageQuery);
  const [data, setData] = useState<UsageSummary | null>(null);
  const [querying, setQuerying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestVersion = useRef(0);

  const run = useCallback(async () => {
    const version = ++requestVersion.current;
    setQuerying(true);
    setError(null);
    try {
      const summary = await queryProfileUsage(profile.id);
      if (requestVersion.current === version) setData(summary);
    } catch (caught) {
      if (requestVersion.current === version) {
        setError((caught as { message?: string }).message ?? "用量查询失败");
      }
    } finally {
      if (requestVersion.current === version) setQuerying(false);
    }
  }, [profile.id]);

  useEffect(() => {
    setData(null);
    void run();
    return () => {
      requestVersion.current += 1;
    };
    // A changed query contract requires a new first read.
  }, [revision, run]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const stop = await onTrayChanged(() => {
          // Read-only: a scheduled or manual query elsewhere just replaced
          // this profile's cache entry.
          void readProfileUsage(profile.id)
            .then((next) => {
              if (!disposed) setData(next);
            })
            .catch(() => undefined);
        });
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      } catch {
        // Without the listener the card still updates on its own queries.
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [profile.id]);

  return { data, querying, error, run };
}

export type ProviderUsage = ReturnType<typeof useProviderUsage>;
