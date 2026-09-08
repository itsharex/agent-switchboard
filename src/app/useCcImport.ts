import { useCallback, useState } from "react";
import {
  importCcswitchProfiles,
  scanCcswitch,
  type CcSwitchImportOutcome,
  type CcSwitchScan,
  type CommandError,
  type AppKind,
  type ProviderRecord,
} from "../api/client";
import { toast } from "../components/use-toast";

interface CcImportDeps {
  busy: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateCandidates: () => void;
  refresh: () => Promise<ProviderRecord[] | undefined>;
  records: ProviderRecord[];
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
      // Fresh scan: select everything importable; exact duplicates stay off.
      const selection: Record<string, boolean> = {};
      for (const item of scan.providers) selection[item.key] = !item.existing;
      setCcSelected(selection);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, onError, setBusy]);

  const runCcImport = useCallback(async () => {
    if (busy || !ccScan) return false;
    const keys = ccScan.providers.filter((item) => ccSelected[item.key]).map((item) => item.key);
    if (keys.length === 0) return false;
    invalidateCandidates();
    setBusy(true);
    clearError();
    try {
      const result = await importCcswitchProfiles(keys);
      setCcResult(result);
      toast({ kind: result.notImported.length > 0 ? "warning" : "success",
        title: `已导入 ${result.importedCount} 项 · 已导入用量脚本 ${result.usageScriptImportedCount} 项`,
        description: result.notImported.length > 0 ? `${result.notImported.length} 项未导入，请查看导入结果` : undefined });
      setCcScan(null);
      setCcSelected({});
      const nextRecords = await refresh();
      if (nextRecords && result.importedCount > 0) {
        const previousIds = new Set(records.map((record) => record.profile.id));
        const added = nextRecords.filter((record) => !previousIds.has(record.profile.id));
        const selected = added.find((record) => record.profile.app === preferredApp) ?? added[0];
        if (selected) {
          setAppFilter(selected.profile.app);
          selectProfile(selected.profile.id);
        }
      }
      return nextRecords !== undefined && result.notImported.length === 0;
    } catch (caught) {
      onError(caught as CommandError);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, ccScan, ccSelected, clearError, invalidateCandidates, onError, refresh, setBusy,
    records, preferredApp, selectProfile, setAppFilter]);

  return { ccScan, ccSelected, setCcSelected, ccResult, runCcScan, runCcImport };
}
