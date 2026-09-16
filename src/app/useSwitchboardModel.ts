import { useCallback, useEffect, useMemo, useState } from "react";
import { onTrayError, onTrayNavigate, openBackupDir, type AppKind, type CommandError } from "../api/client";
import type { Page } from "./navigation";
import { useAppSettings } from "./useAppSettings";
import { useCcImport } from "./useCcImport";
import { useCloudBackup } from "./useCloudBackup";
import { useClientSettings } from "./useClientSettings";
import { useCodexSubagentSettings } from "./useCodexSubagentSettings";
import { useConfigSnapshot } from "./useConfigSnapshot";
import { useDiscovery } from "./useDiscovery";
import { useOperationFrame } from "./useOperationFrame";
import { usePromptDocuments } from "./usePromptDocuments";
import { useProviders } from "./useProviders";
import { latestOverall, useSwitchOperations } from "./useSwitchOperations";
import { useSwitchPreview } from "./useSwitchPreview";
import { useUpdateCheck } from "./useUpdateCheck";
import { useWorkspaceNavigation } from "./useWorkspaceNavigation";

function useTrayEvents(setPage: (page: Page) => void, reportError: (error: CommandError) => void) {
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void onTrayNavigate(() => setPage("供应商")).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    }).catch((error: unknown) => reportError({ code: "TRAY_EVENT", message: error instanceof Error ? error.message : String(error) }));
    return () => { disposed = true; stop?.(); };
  }, [reportError, setPage]);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void onTrayError((message) => reportError({ code: "TRAY_WINDOW", message })).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    }).catch((error: unknown) => reportError({ code: "TRAY_EVENT", message: error instanceof Error ? error.message : String(error) }));
    return () => { disposed = true; stop?.(); };
  }, [reportError]);
}

/** Composes domain hooks; each domain owns its state and typed operations. */
export function useSwitchboardModel() {
  const navigation = useWorkspaceNavigation();
  const { page, setPage, settingsSection, extensionSection } = navigation;
  const [appFilter, setAppFilter] = useState<AppKind>("codex");
  const [requestedCodexPreviewId, setRequestedCodexPreviewId] = useState<string | null>(null);
  const frame = useOperationFrame();
  const { busy, reportError, clearError, setBusy } = frame;
  useTrayEvents(setPage, reportError);
  const operationContext = { busy, onError: reportError, clearError, setBusy };
  const snapshot = useConfigSnapshot({ onError: reportError });
  const { selectedId, setSelectedId, refresh: refreshSnapshot, activeProfileId, records } = snapshot;
  const refresh = useCallback(async () => { await refreshSnapshot(); }, [refreshSnapshot]);
  const switchPreview = useSwitchPreview({ ...operationContext, setSelectedId });
  const { invalidateSwitchCandidates: invalidateCandidates, selectProfile } = switchPreview;
  const clientSettings = useClientSettings({
    ...operationContext, app: appFilter, active: page === "客户端通用配置",
    invalidateSwitchCandidates: invalidateCandidates, refresh,
  });
  const codexSubagentSettings = useCodexSubagentSettings({
    ...operationContext,
    active: page === "客户端通用配置" && appFilter === "codex",
    invalidateSwitchCandidates: invalidateCandidates,
    refresh,
  });
  const promptDocuments = usePromptDocuments({
    ...operationContext, active: page === "扩展" && extensionSection === "instructions",
  });
  const appSettingsState = useAppSettings(operationContext);
  const cloudBackup = useCloudBackup({ ...operationContext, invalidateCandidates, refresh });
  const updateCheck = useUpdateCheck({ onError: reportError });
  const discoveryState = useDiscovery({
    ...operationContext, app: appFilter, invalidateCandidates, refresh: refreshSnapshot, selectProfile, setAppFilter, setPage,
  });
  const ccImport = useCcImport({ ...operationContext, invalidateCandidates, refresh: refreshSnapshot,
    records, codexRecords: snapshot.codexRecords, preferredApp: appFilter, selectProfile, setAppFilter });
  const selectedRecord = records.find((record) => record.profile.id === selectedId) ?? null;
  const selectedProfile = selectedRecord?.profile ?? null;
  const operations = useSwitchOperations({
    ...operationContext, switchCandidate: switchPreview.switchCandidate, retractPreview: switchPreview.retractPreview,
    invalidateCandidates, selectProfile, selectedId, selectedProfile, refresh,
    refreshDiscoveryOrAppend: discoveryState.refreshDiscoveryOrAppend,
  });
  const providers = useProviders({
    ...operationContext, appFilter, setAppFilter, records,
    codexOfficialRecords: snapshot.codexOfficialRecords, selectedId,
    invalidateCandidates, retractPreview: switchPreview.retractPreview, refresh, selectProfile,
    setSelectedId,
  });
  const lastSwitchOverall = useMemo(() => latestOverall(snapshot.statuses ?? []), [snapshot.statuses]);
  const openBackupFolder = useCallback(
    () => openBackupDir().catch((caught) => reportError(caught as CommandError)), [reportError],
  );
  const clearRequestedCodexPreview = useCallback(() => setRequestedCodexPreviewId(null), []);

  return { ...navigation, appFilter, ...frame, snapshot, activeProfileId, switchPreview,
    clientSettings, codexSubagentSettings, promptDocuments, appSettingsState, cloudBackup, updateCheck, discoveryState,
    ccImport, selectedProfile, operations, providers, lastSwitchOverall, openBackupFolder, requestedCodexPreviewId,
    clearRequestedCodexPreview };
}

export type SwitchboardModel = ReturnType<typeof useSwitchboardModel>;
