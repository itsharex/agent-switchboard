import { createElement, useCallback, useState } from "react";
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
import { toast, toastMessage } from "../components/use-toast";
import { CcOutcomeText, ccOutcomeNeedsAttention } from "./cc-import-outcome";
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
  setTargetProfile: (id: string) => void;
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
  setTargetProfile,
  setAppFilter,
}: CcImportDeps) {
  const [ccScan, setCcScan] = useState<CcSwitchScan | null>(null);
  const [ccSelected, setCcSelected] = useState<Record<string, boolean>>({});
  const [ccResult, setCcResult] = useState<CcSwitchImportOutcome | null>(null);
  const [ccDirectory, setCcDirectory] = useState<string | null>(null);

  const runCcScan = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    clearError();
    try {
      const scan = await scanCcswitch(ccDirectory);
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
  }, [busy, ccDirectory, clearError, onError, setBusy]);

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
      const result = await importCcswitchClaudeProfiles(keys, ccDirectory);
      setCcResult(result);
      const needsAttention = ccOutcomeNeedsAttention(result);
      toast({ kind: needsAttention ? "warning" : "success",
        title: createElement(CcOutcomeText, { result }),
        description: result.notImported.length > 0 ? toastMessage("importDiscovery.toast.notImported", { count: result.notImported.length }) : undefined });
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
          setTargetProfile(selected.id);
        }
      }
      return nextInventory !== undefined && !needsAttention;
    } catch (caught) {
      onError(caught as CommandError);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, ccDirectory, ccScan, ccSelected, clearError, invalidateCandidates, onError, refresh, setBusy,
    records, codexRecords, preferredApp, setTargetProfile, setAppFilter]);

  /** Changing the source folder voids the previewed scan; the import's
   * freshness guard would reject its keys against the new database anyway. */
  const changeCcDirectory = useCallback((directory: string | null) => {
    setCcDirectory(directory);
    setCcScan(null);
    setCcSelected({});
  }, []);

  return { ccScan, ccSelected, setCcSelected, ccResult, ccDirectory, changeCcDirectory, runCcScan, runCcImport };
}
