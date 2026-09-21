import { useCallback, useEffect, useState } from "react";
import {
  listRuntimeLogs,
  openRuntimeLogDir,
  type CommandError,
  type RuntimeLogEntry,
  type RuntimeLogLevel,
  type RuntimeLogSeverity,
} from "../api/client";

export type RuntimeLogFilter = "all" | RuntimeLogSeverity;

export const LEVEL_FILTERS: ReadonlyArray<{ value: RuntimeLogFilter; label: string }> = [
  { value: "all", label: "全部" },
  { value: "debug", label: "调试" },
  { value: "info", label: "信息" },
  { value: "warn", label: "警告" },
  { value: "error", label: "错误" },
];

export const LOG_LEVEL_OPTIONS: ReadonlyArray<{ value: RuntimeLogLevel; label: string }> = [
  { value: "debug", label: "调试" },
  { value: "info", label: "信息" },
  { value: "warn", label: "警告" },
  { value: "error", label: "错误" },
  { value: "silent", label: "静默" },
];

export function levelLabel(level: RuntimeLogSeverity): string {
  switch (level) {
    case "debug":
      return "调试";
    case "info":
      return "信息";
    case "warn":
      return "警告";
    case "error":
      return "错误";
  }
}

/** The log table's page size, matching the codebase's page-size convention. */
export const RUNTIME_LOG_PAGE_SIZE = 20;

/** Reads the application's own bounded diagnostic event files. The first read
 * waits until the logs view is first opened; afterwards the entries stay
 * mounted with the page while only their visibility toggles. */
export function useRuntimeLogs(enabled: boolean) {
  const [entries, setEntries] = useState<RuntimeLogEntry[]>([]);
  const [filter, setFilter] = useState<RuntimeLogFilter>("all");
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<CommandError | null>(null);
  const [openingFolder, setOpeningFolder] = useState(false);
  const [folderError, setFolderError] = useState<CommandError | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await listRuntimeLogs());
    } catch (caught) {
      setError(caught as CommandError);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!enabled) return undefined;
    void refresh();
    return undefined;
  }, [enabled, refresh]);

  const openLogDirectory = useCallback(async () => {
    setOpeningFolder(true);
    setFolderError(null);
    try {
      await openRuntimeLogDir();
    } catch (caught) {
      setFolderError(caught as CommandError);
    } finally {
      setOpeningFolder(false);
    }
  }, []);

  /** A new filter is a new result set: the view returns to the first page. */
  const applyFilter = useCallback((next: RuntimeLogFilter) => {
    setFilter(next);
    setPage(1);
  }, []);

  const visibleEntries = filter === "all" ? entries : entries.filter((entry) => entry.level === filter);
  // A refresh can shrink the result set below the stored page; the rendered
  // page converges instead of showing an empty slice.
  const pageCount = Math.max(1, Math.ceil(visibleEntries.length / RUNTIME_LOG_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const pageEntries = visibleEntries.slice(
    (currentPage - 1) * RUNTIME_LOG_PAGE_SIZE,
    currentPage * RUNTIME_LOG_PAGE_SIZE,
  );

  return {
    entries,
    visibleEntries,
    pageEntries,
    page: currentPage,
    setPage,
    filter,
    setFilter: applyFilter,
    loading,
    error,
    refresh,
    openingFolder,
    folderError,
    openLogDirectory,
  };
}
