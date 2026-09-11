import { useCallback, useState } from "react";
import {
  importCcswitchClaudeProfiles,
  scanCcswitch,
  type CcSwitchImportOutcome,
  type CcSwitchScan,
  type CommandError,
  type AppKind,
  type CodexProviderRecord,
  type ProviderRecord,
} from "../api/client";
import { toast } from "../components/use-toast";
import type { ProviderInventory } from "./useConfigSnapshot";

interface CcImportDeps {
  busy: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateCandidates: () => void;
  refresh: () => Promise<ProviderInventory | undefined>;
  records: ProviderRecord[];
  codexRecords: CodexProviderRecord[];
  preferredApp: AppKind;
  selectProfile: (id: string) => void;
  setAppFilter: (app: AppKind) => void;
}

/**
 * Read-only scanning and selection import. API keys never cross
 * the scan boundary; import re-resolves selected source rows in the backend.
 */
export function useCcImport({
  busy,
  onError,
  clearError,
  setBusy,
  invalidateCandidates,
  refresh,
  records,
  codexRecords,
  preferredApp,
  selectProfile,
  setAppFilter,
}: CcImportDeps) {
  const [ccScan, setCcScan] = useState<CcSwitchScan | null>(null);
  const [ccSelected, setCcSelected] = useState<Record<string, boolean>>({});
  const [ccResult, setCcResult] = useState<CcSwitchImportOutcome | null>(null);

  const runCcScan = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    clearError();
    try {
      const scan = await scanCcswitch();
      setCcScan(scan);
      setCcResult(null);
      // Fresh scan: batch-select every importable row (Claude, Codex
      // third-party, and the Codex official record); duplicates stay off.
      const selection: Record<string, boolean> = {};
      for (const item of scan.providers) {
        selection[item.key] = !item.existing;
      }
      setCcSelected(selection);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, onError, setBusy]);

  const runCcImport = useCallback(async () => {
    if (busy || !ccScan) return false;
    const keys = ccScan.providers
      .filter((item) => !item.existing && ccSelected[item.key])
      .map((item) => item.key);
    if (keys.length === 0) return false;
    invalidateCandidates();
    setBusy(true);
    clearError();
    try {
      const result = await importCcswitchClaudeProfiles(keys);
      setCcResult(result);
      toast({ kind: result.notImported.length > 0 ? "warning" : "success",
        title: `已导入 ${result.importedCount} 项 · 已导入用量脚本 ${result.usageScriptImportedCount} 项`,
        description: result.notImported.length > 0 ? `${result.notImported.length} 项未导入，请查看导入结果` : undefined });
      setCcScan(null);
      setCcSelected({});
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
          selectProfile(selected.id);
        }
      }
      return nextInventory !== undefined && result.notImported.length === 0;
    } catch (caught) {
      onError(caught as CommandError);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, ccScan, ccSelected, clearError, invalidateCandidates, onError, refresh, setBusy,
    records, codexRecords, preferredApp, selectProfile, setAppFilter]);

  return { ccScan, ccSelected, setCcSelected, ccResult, runCcScan, runCcImport };
}
