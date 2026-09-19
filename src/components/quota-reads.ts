import { useEffect, useRef, useState } from "react";
import {
  checkCodexResetStatus,
  getCachedCodexOfficialReset,
  getCachedCodexResetStatus,
  refreshCodexOfficialReset,
  type CodexOfficialQuota,
  type CodexOfficialQuotaStatus,
  type CodexResetRead,
} from "../api/client";
import { useUsageHistory } from "./use-usage-history";

function errorMessage(reason: unknown): string {
  return reason instanceof Error && reason.message ? reason.message : "未提供具体原因";
}

function statusCopy(status: Exclude<CodexOfficialQuotaStatus, "available">): string {
  switch (status) {
    case "signInRequired":
      return "未检测到可用的 Codex 官方登录。请完成登录后刷新。";
    case "reauthenticationRequired":
      return "Codex 官方登录已失效。请重新登录后刷新。";
    case "unavailable":
      return "暂时无法读取官方额度，请稍后刷新。";
  }
}

/** The machine's own Codex official quota read: cache first, explicit refresh
 * from the page header. Only an available read is displayable; every other
 * shape (including a null cache) renders the empty state. */
export function useCodexOfficialReset(enabled: boolean) {
  const history = useUsageHistory({ kind: "official" }, enabled);
  const [snapshot, setSnapshot] = useState<CodexOfficialQuota | null>(null);
  const [freshness, setFreshness] = useState<"cached" | "live">("cached");
  const [cacheLoading, setCacheLoading] = useState(true);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [statusNotice, setStatusNotice] = useState<string | null>(null);
  const requestRef = useRef<Promise<CodexOfficialQuota> | null>(null);
  const cacheRevisionRef = useRef(0);

  useEffect(() => {
    if (!enabled) return undefined;
    let active = true;
    const revision = ++cacheRevisionRef.current;

    const loadCache = async () => {
      try {
        const cached = await getCachedCodexOfficialReset();
        if (active && cacheRevisionRef.current === revision) setSnapshot(cached);
      } catch (reason) {
        if (active && cacheRevisionRef.current === revision) setCacheError(errorMessage(reason));
      } finally {
        if (active && cacheRevisionRef.current === revision) setCacheLoading(false);
      }
    };

    void loadCache();
    return () => {
      active = false;
    };
  }, [enabled]);

  const readStatus = async () => {
    if (requestRef.current !== null) return;

    cacheRevisionRef.current += 1;
    setCacheLoading(false);
    setCacheError(null);
    setLoading(true);
    setReadError(null);
    setStatusNotice(null);
    const request = Promise.resolve().then(() => refreshCodexOfficialReset());
    requestRef.current = request;

    try {
      const next = await request;
      if (next.status === "available") {
        setSnapshot(next);
        setFreshness("live");
        void history.refresh();
      } else {
        setStatusNotice(statusCopy(next.status));
      }
    } catch (reason) {
      setReadError(errorMessage(reason));
    } finally {
      if (requestRef.current === request) requestRef.current = null;
      setLoading(false);
    }
  };

  const quota = snapshot !== null && snapshot.status === "available" ? snapshot : null;

  return {
    quota,
    freshness,
    cacheLoading,
    cacheError,
    loading,
    readError,
    statusNotice,
    readStatus,
    history,
  };
}

/** An explicit, read-only read of public reset signals: cache first, explicit
 * refresh from the page header. */
export function useCodexResetSignal(enabled: boolean) {
  const [snapshot, setSnapshot] = useState<CodexResetRead | null>(null);
  const [cacheLoading, setCacheLoading] = useState(true);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const requestRef = useRef<Promise<CodexResetRead> | null>(null);
  const cacheRevisionRef = useRef(0);

  useEffect(() => {
    if (!enabled) return undefined;
    let active = true;
    const revision = ++cacheRevisionRef.current;

    const loadCache = async () => {
      try {
        const cached = await getCachedCodexResetStatus();
        if (active && cacheRevisionRef.current === revision) setSnapshot(cached);
      } catch (reason) {
        if (active && cacheRevisionRef.current === revision) setCacheError(errorMessage(reason));
      } finally {
        if (active && cacheRevisionRef.current === revision) setCacheLoading(false);
      }
    };

    void loadCache();
    return () => {
      active = false;
    };
  }, [enabled]);

  const readStatus = async () => {
    if (requestRef.current !== null) return;

    cacheRevisionRef.current += 1;
    setCacheLoading(false);
    setCacheError(null);
    setLoading(true);
    setReadError(null);
    const request = Promise.resolve().then(() => checkCodexResetStatus());
    requestRef.current = request;

    try {
      setSnapshot(await request);
    } catch (reason) {
      setReadError(errorMessage(reason));
    } finally {
      if (requestRef.current === request) requestRef.current = null;
      setLoading(false);
    }
  };

  return { snapshot, cacheLoading, cacheError, loading, readError, readStatus };
}
