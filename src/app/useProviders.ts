import { useCallback, useState } from "react";
import {
  commitProfileSave,
  deleteProfile,
  prepareProfileSave,
  reorderProfiles,
  resetProfileStore,
  type AppKind,
  type CommandError,
  type FilePreview,
  type ProviderDraft,
  type ProviderProfile,
  type ProviderRecord,
  type UsageQuery,
} from "../api/client";

export type EditorMode = "new" | "edit" | null;

interface ProvidersDeps {
  busy: boolean;
  appFilter: AppKind;
  setAppFilter: (app: AppKind) => void;
  /** Storage revision of the provider file being edited; a save refuses to
   * overwrite a file changed outside the application. */
  selectedRecord: ProviderRecord | null;
  records: ProviderRecord[];
  selectedId: string | null;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateCandidates: () => void;
  retractPreview: () => void;
  refresh: () => Promise<void>;
  selectProfile: (profileId: string) => Promise<void> | void;
  setRecords: (records: ProviderRecord[]) => void;
  setSelectedId: (id: string | null) => void;
}

/**
 * Provider-profile store operations: editor navigation, create, update,
 * delete, reset, and drag reorder. Store writes never touch live client
 * configuration.
 */
export function useProviders({
  busy,
  appFilter,
  setAppFilter,
  selectedRecord,
  records,
  selectedId,
  onError,
  clearError,
  setBusy,
  invalidateCandidates,
  retractPreview,
  refresh,
  selectProfile,
  setRecords,
  setSelectedId,
}: ProvidersDeps) {
  const [editorMode, setEditorMode] = useState<EditorMode>(null);
  const [deletePending, setDeletePending] = useState<ProviderProfile | null>(null);
  const [resetStorePending, setResetStorePending] = useState(false);
  const [pendingSave, setPendingSave] = useState<{
    preparationId: string;
    preview: FilePreview;
  } | null>(null);

  const openEditor = useCallback(
    (profile: ProviderProfile) => {
      retractPreview();
      setSelectedId(profile.id);
      setEditorMode("edit");
    },
    [retractPreview, setSelectedId],
  );

  /** Switching the visible client retracts the preview and clears the
   * selection; nothing is carried across clients. */
  const selectApp = useCallback(
    (app: AppKind) => {
      retractPreview();
      setAppFilter(app);
      setSelectedId(null);
    },
    [retractPreview, setAppFilter, setSelectedId],
  );

  /** Persists a drag reorder of the visible client's provider files. Each
   * file carries its own sort position; the returned list is the same source
   * of truth after the write. */
  const dragReorderProfiles = useCallback(
    async (orderedIds: string[]) => {
      if (busy) return;
      const expectedFileHashes = Object.fromEntries(
        records
          .filter((record) => record.profile.app === appFilter)
          .map((record) => [record.profile.id, record.fileHash]),
      );
      setBusy(true);
      clearError();
      try {
        setRecords(await reorderProfiles(appFilter, orderedIds, expectedFileHashes));
      } catch (caught) {
        onError(caught as CommandError);
      } finally {
        setBusy(false);
      }
    },
    [appFilter, busy, clearError, onError, records, setBusy, setRecords],
  );

  const saveProfile = useCallback(
    async (draft: ProviderDraft) => {
      if (busy) return;
      setBusy(true);
      clearError();
      try {
        const editing = editorMode === "edit" && selectedRecord;
        const prepared = await prepareProfileSave(
          editing ? selectedRecord.profile.id : null,
          draft,
          editing ? selectedRecord.fileHash : null,
        );
        if (prepared.kind === "saveAndApply") {
          if (!prepared.preview) {
            throw { code: "profile-preview-missing", message: "无法生成供应商变更预览" };
          }
          setPendingSave({ preparationId: prepared.preparationId, preview: prepared.preview });
          return;
        }
        const saved = await commitProfileSave(prepared.preparationId, false);
        if (prepared.kind !== "noChange") {
          invalidateCandidates();
        }
        setAppFilter(saved.profile.app);
        setEditorMode(null);
        await refresh();
        await selectProfile(saved.profile.id);
      } catch (caught) {
        onError(caught as CommandError);
      } finally {
        setBusy(false);
      }
    },
    [
      busy,
      clearError,
      editorMode,
      invalidateCandidates,
      onError,
      refresh,
      selectProfile,
      selectedRecord,
      setAppFilter,
      setBusy,
    ],
  );

  const runPendingSave = useCallback(async () => {
    if (busy || !pendingSave) return;
    const pending = pendingSave;
    setPendingSave(null);
    setBusy(true);
    clearError();
    try {
      const saved = await commitProfileSave(pending.preparationId, true);
      invalidateCandidates();
      setAppFilter(saved.profile.app);
      setEditorMode(null);
      await refresh();
      await selectProfile(saved.profile.id);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [
    busy,
    clearError,
    invalidateCandidates,
    onError,
    pendingSave,
    refresh,
    selectProfile,
    setAppFilter,
    setBusy,
  ]);

  /** Persists one metadata-only patch (usage query, quota interval) over a
   * stored profile. Only application-side fields may flow through here: a
   * patch must never re-apply the client configuration. */
  const saveProfilePatch = useCallback(
    async (profile: ProviderProfile, patch: Partial<ProviderDraft>): Promise<boolean> => {
      if (busy) return false;
      const record = records.find((candidate) => candidate.profile.id === profile.id);
      if (!record) return false;
      setBusy(true);
      clearError();
      try {
        const { id, ...draft } = profile;
        const prepared = await prepareProfileSave(id, { ...draft, ...patch }, record.fileHash);
        if (prepared.kind === "saveAndApply") {
          throw {
            code: "profile-save-invalid",
            message: "此修改只允许保存供应商资料，不能应用客户端配置",
          };
        }
        await commitProfileSave(prepared.preparationId, false);
        await refresh();
        return true;
      } catch (caught) {
        onError(caught as CommandError);
        return false;
      } finally {
        setBusy(false);
      }
    },
    [busy, clearError, onError, records, refresh, setBusy],
  );

  const saveProfileUsageQuery = useCallback(
    (profile: ProviderProfile, usageQuery: UsageQuery | null) =>
      saveProfilePatch(profile, { usageQuery }),
    [saveProfilePatch],
  );

  const saveOfficialQuotaInterval = useCallback(
    (profile: ProviderProfile, minutes: number) =>
      saveProfilePatch(profile, { officialQuotaRefreshIntervalMinutes: minutes > 0 ? minutes : null }),
    [saveProfilePatch],
  );

  const runDelete = useCallback(async () => {
    if (busy || !deletePending) return;
    const target = deletePending;
    const record = records.find((candidate) => candidate.profile.id === target.id);
    setDeletePending(null);
    if (!record) {
      onError({ code: "profile-not-found", message: "供应商已不存在，请重新读取" });
      return;
    }
    invalidateCandidates();
    setBusy(true);
    clearError();
    try {
      await deleteProfile(target.id, record.fileHash);
      if (selectedId === target.id) {
        setSelectedId(null);
      }
      setEditorMode(null);
      await refresh();
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [
    busy,
    clearError,
    deletePending,
    invalidateCandidates,
    onError,
    records,
    refresh,
    selectedId,
    setBusy,
    setSelectedId,
  ]);

  const runResetStore = useCallback(async () => {
    if (busy || !resetStorePending) return;
    setResetStorePending(false);
    invalidateCandidates();
    setBusy(true);
    clearError();
    setSelectedId(null);
    setEditorMode(null);
    try {
      await resetProfileStore(true);
      await refresh();
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [
    busy,
    clearError,
    invalidateCandidates,
    onError,
    refresh,
    resetStorePending,
    setBusy,
    setSelectedId,
  ]);

  return {
    editorMode,
    setEditorMode,
    deletePending,
    setDeletePending,
    resetStorePending,
    setResetStorePending,
    pendingSave,
    setPendingSave,
    openEditor,
    selectApp,
    dragReorderProfiles,
    saveProfile,
    runPendingSave,
    saveProfileUsageQuery,
    saveOfficialQuotaInterval,
    runDelete,
    runResetStore,
  };
}
