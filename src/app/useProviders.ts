import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
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

export interface ProviderEditorSession {
  app: AppKind;
  /** The record and storage revision captured when this editor opened. */
  record: ProviderRecord | null;
}

type SetEditorSession = Dispatch<SetStateAction<ProviderEditorSession | null>>;

interface ProvidersDeps {
  busy: boolean;
  appFilter: AppKind;
  setAppFilter: (app: AppKind) => void;
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

function useProviderEditorSession(deps: ProvidersDeps) {
  const { appFilter, records, retractPreview, setAppFilter, setSelectedId } = deps;
  const [editorSession, setEditorSession] = useState<ProviderEditorSession | null>(null);

  const openEditor = useCallback(
    (profile: ProviderProfile) => {
      const record = records.find((item) => item.profile.id === profile.id);
      if (!record) return;
      retractPreview();
      setSelectedId(profile.id);
      setEditorSession({ app: profile.app, record });
    },
    [records, retractPreview, setSelectedId],
  );
  const newEditor = useCallback(() => {
    retractPreview();
    setEditorSession({ app: appFilter, record: null });
  }, [appFilter, retractPreview]);
  const closeEditor = useCallback(() => setEditorSession(null), []);

  /** Changing the list client clears its selection and preview, not the editor session. */
  const selectApp = useCallback(
    (app: AppKind) => {
      retractPreview();
      setAppFilter(app);
      setSelectedId(null);
    },
    [retractPreview, setAppFilter, setSelectedId],
  );

  return { editorSession, setEditorSession, openEditor, newEditor, closeEditor, selectApp };
}

function useProviderSaves(
  deps: ProvidersDeps, editorSession: ProviderEditorSession | null, setEditorSession: SetEditorSession,
) {
  const { busy, clearError, invalidateCandidates, onError, refresh, selectProfile, setAppFilter, setBusy } = deps;
  const [pendingSave, setPendingSave] = useState<{ preparationId: string; preview: FilePreview } | null>(null);
  const saveProfile = useCallback(async (draft: ProviderDraft) => {
    if (busy || !editorSession) return;
    setBusy(true);
    clearError();
    try {
      const record = editorSession.record;
      const prepared = await prepareProfileSave(
        record?.profile.id ?? null,
        draft,
        record?.fileHash ?? null,
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
      setEditorSession(null);
      await refresh();
      await selectProfile(saved.profile.id);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, editorSession, invalidateCandidates, onError, refresh,
    selectProfile, setAppFilter, setBusy, setEditorSession]);

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
      setEditorSession(null);
      await refresh();
      await selectProfile(saved.profile.id);
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, invalidateCandidates, onError, pendingSave, refresh,
    selectProfile, setAppFilter, setBusy, setEditorSession]);
  return { pendingSave, setPendingSave, saveProfile, runPendingSave };
}

function useProviderMetadata(deps: ProvidersDeps) {
  const { appFilter, busy, clearError, onError, records, refresh, setBusy, setRecords } = deps;
  const dragReorderProfiles = useCallback(async (orderedIds: string[]) => {
    if (busy) return;
    const expectedFileHashes = Object.fromEntries(records.filter((record) => record.profile.app === appFilter)
      .map((record) => [record.profile.id, record.fileHash]));
    setBusy(true);
    clearError();
    try {
      setRecords(await reorderProfiles(appFilter, orderedIds, expectedFileHashes));
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [appFilter, busy, clearError, onError, records, setBusy, setRecords]);
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
  return { dragReorderProfiles, saveProfileUsageQuery, saveOfficialQuotaInterval };
}

function useProviderRemoval(deps: ProvidersDeps, setEditorSession: SetEditorSession) {
  const { busy, clearError, invalidateCandidates, onError, records, refresh, selectedId, setBusy, setSelectedId } = deps;
  const [deletePending, setDeletePending] = useState<ProviderProfile | null>(null);
  const [resetStorePending, setResetStorePending] = useState(false);
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
      setEditorSession((current) => current?.record?.profile.id === target.id ? null : current);
      await refresh();
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, deletePending, invalidateCandidates, onError, records,
    refresh, selectedId, setBusy, setSelectedId, setEditorSession]);

  const runResetStore = useCallback(async () => {
    if (busy || !resetStorePending) return;
    setResetStorePending(false);
    invalidateCandidates();
    setBusy(true);
    clearError();
    setSelectedId(null);
    setEditorSession(null);
    try {
      await resetProfileStore(true);
      await refresh();
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, invalidateCandidates, onError, refresh,
    resetStorePending, setBusy, setSelectedId, setEditorSession]);
  return { deletePending, setDeletePending, resetStorePending, setResetStorePending, runDelete, runResetStore };
}

/** Editor identity remains independent of the current list selection and refreshed records. */
export function useProviders(deps: ProvidersDeps) {
  const { setEditorSession, ...editor } = useProviderEditorSession(deps);
  const saves = useProviderSaves(deps, editor.editorSession, setEditorSession);
  const metadata = useProviderMetadata(deps);
  const removal = useProviderRemoval(deps, setEditorSession);
  return { ...editor, ...saves, ...metadata, ...removal };
}
