import type { MessageKey } from "../i18n";
import { useCallback, useEffect, useState } from "react";
import type { LocalizedMessage } from "../api/client";
import {
  discoverCached,
  discoverLocal,
  importDiscoveredCodexProfile,
  importDiscoveredClaudeProfile,
  type AppKind,
  type CommandError,
  type DiscoveryReport,
} from "../api/client";
import { toast, toastMessage } from "../components/use-toast";
import type { ProviderInventory } from "./useConfigSnapshot";

interface DiscoveryDeps {
  app: AppKind;
  busy: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateCandidates: () => void;
  refresh: () => Promise<ProviderInventory | undefined>;
  setTargetProfile: (profileId: string) => Promise<void> | void;
  setAppFilter: (app: AppKind) => void;
  setPage: (page: "providers") => void;
}

/**
 * Local configuration discovery: read-only scanning plus importing a
 * discovered provider into the profile store. Switch operations refresh the
 * discovery result after each write so the import view never goes stale.
 */
export function useDiscovery({
  app,
  busy,
  onError,
  clearError,
  setBusy,
  invalidateCandidates,
  refresh,
  setTargetProfile,
  setAppFilter,
  setPage,
}: DiscoveryDeps) {
  const [discovery, setDiscovery] = useState<DiscoveryReport | null>(null);

  /** Shows the previous scan while the page loads; an absent or unreadable
   * cache simply means no scan has run yet, not an error. */
  useEffect(() => {
    let active = true;
    discoverCached()
      .then((cached) => {
        if (active && cached) setDiscovery(cached);
      })
      .catch(() => {
        /* 缓存不可读等同于"还没有上次扫描"，不作为错误上报 */
      });
    return () => {
      active = false;
    };
  }, []);

  /** Refreshes discovery after a successful write and returns the warnings
   * to report: the incoming ones, plus a note when the refresh itself
   * failed. The write still succeeded, so a failure is a warning, not an
   * error. */
  const refreshDiscoveryOrAppend = useCallback(
    async (warnings: readonly LocalizedMessage[], failureNote: MessageKey): Promise<LocalizedMessage[]> => {
      try {
        setDiscovery(await discoverLocal());
        return [...warnings];
      } catch {
        return [...warnings, { key: failureNote, text: "" }];
      }
    },
    [],
  );

  const runDiscovery = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    clearError();
    try {
      setDiscovery(await discoverLocal());
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, onError, setBusy]);

  const runImport = useCallback(
    async () => {
      if (busy) return false;
      invalidateCandidates();
      setBusy(true);
      clearError();
      try {
        const result = app === "codex"
          ? await importDiscoveredCodexProfile()
          : await importDiscoveredClaudeProfile();
        setDiscovery(null);
        toast({ kind: "success", title: toastMessage("importDiscovery.toast.importedProvider", { name: "profile" in result ? result.profile.name : result.name }) });
        const refreshed = await refresh();
        if (!refreshed) return false;
        setAppFilter(app);
        setPage("providers");
        await setTargetProfile("profile" in result ? result.profile.id : result.id);
        return true;
      } catch (caught) {
        onError(caught as CommandError);
        return false;
      } finally {
        setBusy(false);
      }
    },
    [
      app,
      busy,
      clearError,
      invalidateCandidates,
      onError,
      refresh,
      setTargetProfile,
      setAppFilter,
      setBusy,
      setPage,
    ],
  );

  return { discovery, runDiscovery, runImport, refreshDiscoveryOrAppend };
}
