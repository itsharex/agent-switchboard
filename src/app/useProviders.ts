import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
import {
  commitCodexProfileSave,
  commitProfileSave,
  createCodexProfile,
  deleteCodexProfile,
  deleteProfile,
  prepareCodexProfileSave,
  prepareProfileSave,
  reorderProfiles,
  resetProfileStore,
  type AppKind,
  type CommandError,
  type CodexProviderDraft,
  type CodexProviderRecord,
  type FilePreview,
  type ProviderDraft,
  type ProviderProfile,
  type ProviderRecord,
  type UsageQuery,
} from "../api/client";

/** What a Codex editor session edits: a stored third-party record or the
 * client's official-login record. The variants never convert into each other;
 * only their shared editor shape is editable. */
export type CodexEditorSource =
  | { kind: "record"; record: CodexProviderRecord }
  | { kind: "official"; record: ProviderRecord | null };

/** One open editor. Claude owns the generic contract; Codex owns its strict
 * specialized contract — the two draft shapes never convert into each other. */
export type ProviderEditorSession =
  | { app: "claude"; record: ProviderRecord | null }
  | { app: "codex"; source: CodexEditorSource | null };

function codexSessionRecord(session: ProviderEditorSession | null): CodexProviderRecord | null {
  if (session?.app !== "codex") return null;
  return session.source?.kind === "record" ? session.source.record : null;
}

type SetEditorSession = Dispatch<SetStateAction<ProviderEditorSession | null>>;

interface ProvidersDeps {
  busy: boolean;
  appFilter: AppKind;
  setAppFilter: (app: AppKind) => void;
  records: ProviderRecord[];
  /** The Codex official-login record, stored in the same generic boundary. */
  codexOfficialRecords: ProviderRecord[];
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
      setEditorSession({ app: "claude", record });
    },
    [records, retractPreview, setSelectedId],
  );
  const openCodexEditor = useCallback(
    (record: CodexProviderRecord) => {
      retractPreview();
      setSelectedId(record.profile.id);
      setEditorSession({ app: "codex", source: { kind: "record", record } });
    },
    [retractPreview, setSelectedId],
  );
  /** Opens the Codex editor on the client's official-login record. A null
   * record creates it; the record itself is never a third-party draft. */
  const openCodexOfficialEditor = useCallback(
    (record: ProviderRecord | null) => {
      retractPreview();
      setSelectedId(record?.profile.id ?? null);
      setAppFilter("codex");
      setEditorSession({ app: "codex", source: { kind: "official", record } });
    },
    [retractPreview, setAppFilter, setSelectedId],
  );
  /** Switches an open Codex editor between its two access modes by replacing
   * the session; a third-party draft never mutates into an official record. */
  const switchCodexAccessMode = useCallback(
    (official: boolean, existing: ProviderRecord | null) => {
      setSelectedId(existing?.profile.id ?? null);
      setEditorSession({
        app: "codex",
        source: official
          ? { kind: "official", record: existing }
          : null,
      });
    },
    [setSelectedId],
  );
  const newEditor = useCallback(() => {
    retractPreview();
    setEditorSession(
      appFilter === "codex"
        ? { app: "codex", source: null }
        : { app: "claude", record: null },
    );
  }, [appFilter, retractPreview]);
  /** Cross-contract client switch: replace the session instead of mutating
   * one contract's draft into the other's. */
  const newEditorFor = useCallback((app: AppKind) => {
    retractPreview();
    setAppFilter(app);
    setSelectedId(null);
    setEditorSession(
      app === "codex"
        ? { app: "codex", source: null }
        : { app: "claude", record: null },
    );
  }, [retractPreview, setAppFilter, setSelectedId]);
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

  return { editorSession, setEditorSession, openEditor, openCodexEditor, openCodexOfficialEditor,
    switchCodexAccessMode, newEditor, newEditorFor, closeEditor, selectApp };
}

/** One save awaiting its explicit write confirmation. `store` selects the
 * transaction: the generic provider boundary (Claude or the Codex
 * official-login record) or the strict Codex third-party store. */
interface PendingSave {
  kind: "generic" | "codex";
  app: AppKind;
  preparationId: string;
  preview: FilePreview;
}

function useProviderSaves(
  deps: ProvidersDeps, editorSession: ProviderEditorSession | null, setEditorSession: SetEditorSession,
) {
  const { busy, clearError, invalidateCandidates, onError, refresh, selectProfile, setAppFilter, setBusy } = deps;
  const [pendingSave, setPendingSave] = useState<PendingSave | null>(null);
  const saveProfile = useCallback(async (draft: ProviderDraft) => {
    if (busy || !editorSession || editorSession.app !== "claude") return;
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
        setPendingSave({ kind: "generic", app: "claude", preparationId: prepared.preparationId, preview: prepared.preview });
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

  /** Codex saves never migrate through the generic transaction: creation
   * (blank or an import seed) writes the specialized store directly; an edit
   * of the live profile prepares a preview that must be confirmed before it
   * is applied. */
  const saveCodexProfile = useCallback(async (draft: CodexProviderDraft) => {
    if (busy || !editorSession || editorSession.app !== "codex") return;
    setBusy(true);
    clearError();
    try {
      const record = codexSessionRecord(editorSession);
      if (!record) {
        const created = await createCodexProfile(draft);
        invalidateCandidates();
        setAppFilter("codex");
        setEditorSession(null);
        await refresh();
        await selectProfile(created.profile.id);
        return;
      }
      const prepared = await prepareCodexProfileSave(
        record.profile.id,
        draft,
        record.fileHash,
      );
      if (prepared.kind === "saveAndApply") {
        if (!prepared.preview) {
          throw { code: "codex-profile-save-stale", message: "Codex 保存预览缺失，请重新保存" };
        }
        setPendingSave({ kind: "codex", app: "codex", preparationId: prepared.preparationId, preview: prepared.preview });
        return;
      }
      const saved = await commitCodexProfileSave(prepared.preparationId, false);
      if (prepared.kind !== "noChange") {
        invalidateCandidates();
      }
      setAppFilter("codex");
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

  /** The Codex official-login record is an ordinary generic-boundary record:
   * it saves through the same prepare/commit transaction as a Claude profile,
   * never through the strict third-party store. */
  const saveCodexOfficialProfile = useCallback(async (draft: ProviderDraft) => {
    if (busy || !editorSession || editorSession.app !== "codex"
      || editorSession.source?.kind !== "official") return;
    setBusy(true);
    clearError();
    try {
      const record = editorSession.source.record;
      const prepared = await prepareProfileSave(
        record?.profile.id ?? null,
        draft,
        record?.fileHash ?? null,
      );
      if (prepared.kind === "saveAndApply") {
        if (!prepared.preview) {
          throw { code: "profile-preview-missing", message: "无法生成供应商变更预览" };
        }
        setPendingSave({ kind: "generic", app: "codex", preparationId: prepared.preparationId, preview: prepared.preview });
        return;
      }
      const saved = await commitProfileSave(prepared.preparationId, false);
      if (prepared.kind !== "noChange") {
        invalidateCandidates();
      }
      setAppFilter("codex");
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
      const saved = pending.kind === "codex"
        ? await commitCodexProfileSave(pending.preparationId, true)
        : await commitProfileSave(pending.preparationId, true);
      invalidateCandidates();
      setAppFilter(pending.app);
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
  return { pendingSave, setPendingSave, saveProfile, saveCodexProfile, saveCodexOfficialProfile, runPendingSave };
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

/** One pending destructive provider removal. `generic` covers the Claude
 * providers and the Codex official-login record (both live in the generic
 * boundary); `codexThirdParty` covers the strict Codex store. Both clients
 * confirm through the same sheet; nothing deletes without it. */
export type PendingProviderRemoval =
  | { kind: "generic"; profile: ProviderProfile }
  | { kind: "codexThirdParty"; record: CodexProviderRecord };

function useProviderRemoval(deps: ProvidersDeps, setEditorSession: SetEditorSession) {
  const { busy, clearError, codexOfficialRecords, invalidateCandidates, onError, records, refresh,
    selectedId, setBusy, setSelectedId } = deps;
  const [deletePending, setDeletePending] = useState<PendingProviderRemoval | null>(null);
  const [resetStorePending, setResetStorePending] = useState(false);
  const runDelete = useCallback(async () => {
    if (busy || !deletePending) return;
    const target = deletePending;
    setDeletePending(null);
    if (target.kind === "codexThirdParty") {
      const id = target.record.profile.id;
      invalidateCandidates();
      setBusy(true);
      clearError();
      try {
        await deleteCodexProfile(id, target.record.fileHash);
        setEditorSession((current) =>
          codexSessionRecord(current)?.profile.id === id ? null : current);
        await refresh();
      } catch (caught) {
        onError(caught as CommandError);
      } finally {
        setBusy(false);
      }
      return;
    }
    const id = target.profile.id;
    const record = [...records, ...codexOfficialRecords]
      .find((candidate) => candidate.profile.id === id);
    if (!record) {
      onError({ code: "profile-not-found", message: "供应商已不存在，请重新读取" });
      return;
    }
    invalidateCandidates();
    setBusy(true);
    clearError();
    try {
      await deleteProfile(id, record.fileHash);
      if (selectedId === id) {
        setSelectedId(null);
      }
      setEditorSession((current) =>
        current?.app === "codex" && current.source?.kind === "official"
          && current.source.record?.profile.id === id
          ? null
          : current?.app === "claude" && current.record?.profile.id === id
            ? null
            : current);
      await refresh();
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, codexOfficialRecords, deletePending, invalidateCandidates, onError, records,
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
