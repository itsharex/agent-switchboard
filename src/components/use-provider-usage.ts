import { useCallback, useEffect, useRef, useState } from "react";
import {
  ensureProfileUsage,
  onTrayChanged,
  queryProfileUsage,
  readProfileUsage,
  type UsageQuery,
  type UsageSummary,
} from "../api/client";

/** The card owns queries so collapsing its details does not stop them. The
 * backend owns all timing: the mount-time read ensures freshness (the cached
 * entry while not due, one query pulled forward when due), the refresh
 * button forces one query, and tray-changed adopts newer cache entries.
 * Both clients' rows feed it the same minimal facts: the stable profile id
 * and its persisted query. */
export function useProviderUsage(profile: { id: string; usageQuery?: UsageQuery | null }) {
  const revision = JSON.stringify(profile.usageQuery);
  const [data, setData] = useState<UsageSummary | null>(null);
  const [querying, setQuerying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestVersion = useRef(0);

  const request = useCallback(async (mode: "ensure" | "force") => {
    const version = ++requestVersion.current;
    setQuerying(true);
    setError(null);
    try {
      const summary = mode === "force"
        ? await queryProfileUsage(profile.id)
        : await ensureProfileUsage(profile.id);
      if (requestVersion.current === version) setData(summary);
    } catch (caught) {
      if (requestVersion.current === version) {
        setError((caught as { message?: string }).message ?? "用量查询失败");
      }
    } finally {
      if (requestVersion.current === version) setQuerying(false);
    }
  }, [profile.id]);

  const run = useCallback(() => request("force"), [request]);

  useEffect(() => {
    setData(null);
    void request("ensure");
    return () => {
      requestVersion.current += 1;
    };
    // A changed query contract requires a new ensure-fresh read.
  }, [revision, request]);

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
