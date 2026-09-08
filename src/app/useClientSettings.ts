import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import {
  getClientSettingsEditor, previewClientSettings, saveClientSettings,
  type AppKind, type CommandError, type ClientSettingsEditor, type ClientSettingsPreview, type SettingValue,
} from "../api/client";

export type ClientSettingsPhase = "idle" | "loading" | "loadError" | "clean" | "dirty" | "saving" | "saveError" | "savedPendingReapply";

export interface ClientSettingsEditorState {
  phase: ClientSettingsPhase;
  editor?: ClientSettingsEditor;
  draft?: Record<string, SettingValue>;
  error?: CommandError;
  preview?: ClientSettingsPreview;
  previewing?: boolean;
  previewError?: CommandError;
}

interface ClientSettingsDeps {
  app: AppKind;
  busy: boolean;
  active: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  invalidateSwitchCandidates: () => void;
  refresh: () => Promise<void>;
  setBusy: (busy: boolean) => void;
}
type States = Record<AppKind, ClientSettingsEditorState>;
type SetStates = Dispatch<SetStateAction<States>>;

function sameValues(left: Record<string, SettingValue>, right: Record<string, SettingValue>): boolean {
  return Object.keys(left).length === Object.keys(right).length && Object.keys(left).every((key) => {
    const before = left[key];
    const after = right[key];
    return after && before.mode === after.mode &&
      (before.mode === "automatic" || (after.mode === "explicit" && before.value === after.value));
  });
}

function phaseForDraft(editor: ClientSettingsEditor, draft: Record<string, SettingValue>, prior: ClientSettingsPhase): ClientSettingsPhase {
  if (!sameValues(draft, editor.settings.settings)) return "dirty";
  return prior === "savedPendingReapply" ? "savedPendingReapply" : "clean";
}

function changedState(state: ClientSettingsEditorState, draft: Record<string, SettingValue>): ClientSettingsEditorState {
  return { ...state, draft, phase: phaseForDraft(state.editor!, draft, state.phase),
    error: undefined, preview: undefined, previewing: false, previewError: undefined };
}

function useLoadClientSettings(active: boolean, app: AppKind, states: States, setStates: SetStates) {
  const requests = useRef(new Map<AppKind, Promise<ClientSettingsEditor>>());
  const load = useCallback((target: AppKind, preserveDraft = false) => {
    const existing = requests.current.get(target);
    if (existing) return existing;
    setStates((current) => ({ ...current, [target]: { ...current[target], phase: "loading", error: undefined } }));
    const request = getClientSettingsEditor(target);
    requests.current.set(target, request);
    void request.then((editor) => setStates((current) => {
      const prior = current[target];
      const draft = preserveDraft && prior.draft ? prior.draft : editor.settings.settings;
      return { ...current, [target]: { phase: phaseForDraft(editor, draft, prior.phase), editor, draft } };
    })).catch((caught) => setStates((current) => ({
      ...current, [target]: { ...current[target], phase: "loadError", error: caught as CommandError },
    }))).finally(() => {
      if (requests.current.get(target) === request) requests.current.delete(target);
    });
    return request;
  }, [setStates]);
  useEffect(() => {
    if (active && states[app].phase === "idle") void load(app);
  }, [active, app, load, states]);
  return load;
}

function useSaveClientSettings(deps: ClientSettingsDeps, states: States, setStates: SetStates) {
  const { busy, clearError, invalidateSwitchCandidates, onError, refresh, setBusy } = deps;
  return useCallback(async (app: AppKind) => {
    const current = states[app];
    if (busy || !current.editor || !current.draft) return false;
    if (current.phase === "clean" || current.phase === "savedPendingReapply") return true;
    if (current.phase !== "dirty" && current.phase !== "saveError") return false;
    setStates((all) => ({ ...all, [app]: { ...all[app], phase: "saving", error: undefined } }));
    setBusy(true);
    clearError();
    try {
      const saved = await saveClientSettings(app, { settings: current.draft }, current.editor.settingsHash);
      invalidateSwitchCandidates();
      await refresh().catch((caught) => onError(caught as CommandError));
      setStates((all) => ({ ...all, [app]: {
        phase: "savedPendingReapply",
        editor: { ...current.editor!, settings: saved.settings, settingsHash: saved.settingsHash },
        draft: saved.settings.settings,
      } }));
      return true;
    } catch (caught) {
      const error = caught as CommandError;
      setStates((all) => ({ ...all, [app]: { ...all[app], phase: "saveError", error } }));
      onError(error);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, invalidateSwitchCandidates, onError, refresh, setBusy, setStates, states]);
}

function usePreviewClientSettings(busy: boolean, states: States, setStates: SetStates) {
  return useCallback((app: AppKind) => {
    const state = states[app];
    if (busy || state.previewing || !state.draft) return;
    const draft = state.draft;
    setStates((current) => ({ ...current, [app]: { ...current[app], previewing: true, previewError: undefined } }));
    void previewClientSettings(app, { settings: draft }).then((preview) => setStates((current) => {
      const latest = current[app];
      if (!latest.draft || !sameValues(latest.draft, draft)) return current;
      return { ...current, [app]: { ...latest, preview, previewing: false } };
    })).catch((caught) => setStates((current) => {
      const latest = current[app];
      if (!latest.draft || !sameValues(latest.draft, draft)) return current;
      return { ...current, [app]: { ...latest, previewing: false, previewError: caught as CommandError } };
    }));
  }, [busy, states, setStates]);
}

/** Each client preference draft lives in application state until its explicit save. */
export function useClientSettings(deps: ClientSettingsDeps) {
  const { active, app, busy } = deps;
  const [states, setStates] = useState<States>({ codex: { phase: "idle" }, claude: { phase: "idle" } });
  const load = useLoadClientSettings(active, app, states, setStates);
  const saveSettings = useSaveClientSettings(deps, states, setStates);
  const previewSettings = usePreviewClientSettings(busy, states, setStates);
  const changeValue = useCallback((app: AppKind, key: string, value: SettingValue) => {
    if (busy) return;
    setStates((current) => {
      const state = current[app];
      if (!state.editor || !state.draft || state.phase === "saving") return current;
      return { ...current, [app]: changedState(state, { ...state.draft, [key]: value }) };
    });
  }, [busy]);
  const resetGroupToDefaults = useCallback((app: AppKind, group: string | null) => {
    if (busy) return;
    setStates((current) => {
      const state = current[app];
      if (!state.editor || !state.draft || state.phase === "saving") return current;
      const draft = { ...state.draft };
      for (const spec of state.editor.specs) {
        if (group === null || spec.group === group) draft[spec.key] = { mode: "automatic" };
      }
      return { ...current, [app]: changedState(state, draft) };
    });
  }, [busy]);
  const retryLoad = useCallback((app: AppKind) => {
    if (!busy) void load(app, states[app].draft !== undefined);
  }, [busy, load, states]);
  return { editorState: states[app],
    changeValue, resetGroupToDefaults, saveSettings, retryLoad, previewSettings };
}
