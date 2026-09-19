import { useCallback, useState } from "react";
import {
  applyProvidersSql,
  importProvidersSql,
  type CommandError,
  type AppKind,
  type CodexProviderRecord,
  type ProviderRecord,
  type ProviderSqlImportOutcome,
  type ProviderSqlScan,
} from "../api/client";
import { toast } from "../components/use-toast";
import type { ProviderInventory } from "./useConfigSnapshot";

interface SqlImportDeps {
  busy: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateCandidates: () => void;
  refresh: () => Promise<ProviderInventory | undefined>;
  records: ProviderRecord[];
  codexRecords: CodexProviderRecord[];
  preferredApp: AppKind;
  setTargetProfile: (id: string) => void;
  setAppFilter: (app: AppKind) => void;
}

/**
 * Applies one exported SQL file into the app-owned scratch database and
 * imports the previewed selection. Re-applying the same file inside the
 * import is the freshness guard, so a stale preview can never import.
 */
export function useSqlImport({
  busy,
  onError,
  clearError,
  setBusy,
  invalidateCandidates,
  refresh,
  records,
  codexRecords,
  preferredApp,
  setTargetProfile,
  setAppFilter,
}: SqlImportDeps) {
  const [sqlScan, setSqlScan] = useState<ProviderSqlScan | null>(null);
  const [sqlSelected, setSqlSelected] = useState<Record<string, boolean>>({});
  const [sqlResult, setSqlResult] = useState<ProviderSqlImportOutcome | null>(null);
  const [sqlPath, setSqlPath] = useState<string | null>(null);

  const runSqlApply = useCallback(async (path: string) => {
    if (busy) return;
    setBusy(true);
    clearError();
    try {
      const scan = await applyProvidersSql(path);
      setSqlPath(path);
      setSqlScan(scan);
      setSqlResult(null);
      // Fresh preview: batch-select every new row; overwriting an existing
      // provider stays an explicit opt-in.
      const selection: Record<string, boolean> = {};
      for (const item of scan.providers) {
        selection[item.key] = !item.existing;
      }
      setSqlSelected(selection);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, onError, setBusy]);

  const runSqlImport = useCallback(async () => {
    if (busy || !sqlScan || !sqlPath) return false;
    const ids = sqlScan.providers
      .filter((item) => sqlSelected[item.key])
      .map((item) => item.key);
    if (ids.length === 0) return false;
    invalidateCandidates();
    setBusy(true);
    clearError();
    try {
      const result = await importProvidersSql(ids, sqlPath);
      setSqlResult(result);
      toast({ kind: result.notImported.length > 0 ? "warning" : "success",
        title: `已导入 ${result.importedCount} 项${result.updatedCount > 0 ? ` · 覆盖更新 ${result.updatedCount} 项` : ""}`,
        description: result.notImported.length > 0 ? `${result.notImported.length} 项未导入，请查看导入结果` : undefined });
      setSqlScan(null);
      setSqlSelected({});
      const nextInventory = await refresh();
      if (nextInventory && result.importedCount > 0) {
        const previousIds = new Set([...records, ...codexRecords].map((record) => record.profile.id));
        const added = [
          ...nextInventory.claude.map((record) => ({ app: record.profile.app, id: record.profile.id })),
          ...nextInventory.codex.map((record) => ({ app: "codex" as const, id: record.profile.id })),
        ].filter((record) => !previousIds.has(record.id));
        const selected = added.find((record) => record.app === preferredApp) ?? added[0];
        if (selected) {
          setAppFilter(selected.app);
          setTargetProfile(selected.id);
        }
      }
      return nextInventory !== undefined && result.notImported.length === 0;
    } catch (caught) {
      onError(caught as CommandError);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, sqlPath, sqlScan, sqlSelected, clearError, invalidateCandidates, onError, refresh, setBusy,
    records, codexRecords, preferredApp, setTargetProfile, setAppFilter]);

  return { sqlScan, sqlSelected, setSqlSelected, sqlResult, sqlPath, runSqlApply, runSqlImport };
}
