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
import { useProviderSwitchFlow } from "./useProviderSwitchFlow";
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
  const { page, setPage, settingsSection } = navigation;
  const [appFilter, setAppFilter] = useState<AppKind>("codex");
  const frame = useOperationFrame();
  const { busy, reportError, clearError, setBusy } = frame;
  useTrayEvents(setPage, reportError);
  const operationContext = { busy, onError: reportError, clearError, setBusy };
  const snapshot = useConfigSnapshot({ onError: reportError });
  const { targetProfileId, setTargetProfileId, refresh: refreshSnapshot, activeProfileId, records } = snapshot;
  const refresh = useCallback(async () => { await refreshSnapshot(); }, [refreshSnapshot]);
  const providerSwitch = useProviderSwitchFlow({
    busy,
    onError: reportError,
    clearError,
    onTargetProfileChange: setTargetProfileId,
  });
  const { invalidateCandidates, setTargetProfile } = providerSwitch;
  const clientSettings = useClientSettings({
    app: appFilter,
    busy,
    active: page === "客户端通用配置",
  });
  const codexSubagentSettings = useCodexSubagentSettings({
    active: page === "客户端通用配置" && appFilter === "codex",
    busy,
    onError: reportError,
  });
  const promptDocuments = usePromptDocuments({
    ...operationContext, active: page === "客户端通用配置",
  });
  const appSettingsState = useAppSettings(operationContext);
  const cloudBackup = useCloudBackup({ ...operationContext, invalidateCandidates, refresh });
  const updateCheck = useUpdateCheck({ onError: reportError });
  const discoveryState = useDiscovery({
    ...operationContext, app: appFilter, invalidateCandidates, refresh: refreshSnapshot, setTargetProfile, setAppFilter, setPage,
  });
  const ccImport = useCcImport({ ...operationContext, invalidateCandidates, refresh: refreshSnapshot,
    records, codexRecords: snapshot.codexRecords, preferredApp: appFilter, setTargetProfile, setAppFilter });
  const targetRecord = records.find((record) => record.profile.id === targetProfileId) ?? null;
  const targetProfile = targetRecord?.profile ?? null;
  const operations = useSwitchOperations({
    ...operationContext, activationCandidate: providerSwitch.activationCandidate, clearCandidates: providerSwitch.clearCandidates,
    invalidateCandidates, setTargetProfile, targetProfileId, targetProfile, refresh,
    refreshDiscoveryOrAppend: discoveryState.refreshDiscoveryOrAppend,
  });
  const providers = useProviders({
    ...operationContext, appFilter, setAppFilter, records,
    codexOfficialRecords: snapshot.codexOfficialRecords, targetProfileId,
    invalidateCandidates, clearCandidates: providerSwitch.clearCandidates, refresh, setTargetProfile,
    setTargetProfileId,
  });
  const lastSwitchOverall = useMemo(() => latestOverall(snapshot.statuses ?? []), [snapshot.statuses]);
  const openBackupFolder = useCallback(
    () => openBackupDir().catch((caught) => reportError(caught as CommandError)), [reportError],
  );

  return { ...navigation, appFilter, ...frame, snapshot, activeProfileId, providerSwitch,
    clientSettings, codexSubagentSettings, promptDocuments, appSettingsState, cloudBackup, updateCheck, discoveryState,
    ccImport, targetProfile, operations, providers, lastSwitchOverall, openBackupFolder };
}

export type SwitchboardModel = ReturnType<typeof useSwitchboardModel>;
