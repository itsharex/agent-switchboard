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

/** Reads the application's own bounded diagnostic event files. The first read
 * waits until the logs view is first opened; afterwards the entries stay
 * mounted with the page while only their visibility toggles. */
export function useRuntimeLogs(enabled: boolean) {
  const [entries, setEntries] = useState<RuntimeLogEntry[]>([]);
  const [filter, setFilter] = useState<RuntimeLogFilter>("all");
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

  const visibleEntries = filter === "all" ? entries : entries.filter((entry) => entry.level === filter);

  return {
    entries,
    visibleEntries,
    filter,
    setFilter,
    loading,
    error,
    refresh,
    openingFolder,
    folderError,
    openLogDirectory,
  };
}
