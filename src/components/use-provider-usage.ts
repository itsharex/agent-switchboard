import { useCallback, useEffect, useRef, useState } from "react";
import {
  onTrayChanged,
  queryProfileUsage,
  readProfileUsage,
  type ProviderProfile,
  type UsageSummary,
} from "../api/client";
import { useUsageHistory } from "./use-usage-history";

/** The card owns queries so collapsing its details does not stop them. The
 * backend scheduler owns automatic re-query timing; this hook only performs
 * the mount-time first read and manual refreshes, and adopts newer cache
 * entries announced through tray-changed. */
export function useProviderUsage(profile: ProviderProfile) {
  const revision = JSON.stringify(profile.usageQuery);
  const history = useUsageHistory({ kind: "provider", profileId: profile.id }, revision);
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
      if (requestVersion.current === version) {
        setData(summary);
        void history.refresh();
      }
    } catch (caught) {
      if (requestVersion.current === version) {
        setError((caught as { message?: string }).message ?? "用量查询失败");
      }
    } finally {
      if (requestVersion.current === version) setQuerying(false);
    }
  }, [history.refresh, profile.id]);

  useEffect(() => {
    setData(null);
    void run();
    return () => {
      requestVersion.current += 1;
    };
    // The query contract changed: the retained cache no longer applies, so
    // the first read must run again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, revision]);

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

  return { data, querying, error, run, history };
}

export type ProviderUsage = ReturnType<typeof useProviderUsage>;
