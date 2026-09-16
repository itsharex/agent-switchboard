import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { clientSettingsPayload, sameClaudeExtra, type ClaudeExtraSettings } from "./claude-common-settings";
import {
  getClientSettingsEditor, parseClientSettings, previewClientSettings,
  type AppKind, type CommandError, type ClientSettingsEditor, type ClientSettingsPreview,
  type SettingValue,
} from "../api/client";

export type ClientSettingsPhase = "idle" | "loading" | "loadError" | "clean" | "dirty";

export interface ClientSettingsEditorState {
  phase: ClientSettingsPhase;
  editor?: ClientSettingsEditor;
  draft?: Record<string, SettingValue>;
  claudeExtra?: ClaudeExtraSettings;
  error?: CommandError;
  preview?: ClientSettingsPreview;
  previewing?: boolean;
  previewError?: CommandError;
  parsing?: boolean;
  parseError?: CommandError;
}

interface ClientSettingsDeps {
  app: AppKind;
  busy: boolean;
  active: boolean;
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

function phaseForDraft(editor: ClientSettingsEditor, draft: Record<string, SettingValue>, extra: ClaudeExtraSettings): ClientSettingsPhase {
  return !sameValues(draft, editor.settings.settings) || !sameClaudeExtra(extra, editor.settings.claudeExtra)
    ? "dirty" : "clean";
}

function changedState(state: ClientSettingsEditorState, draft: Record<string, SettingValue>, extra: ClaudeExtraSettings = state.claudeExtra): ClientSettingsEditorState {
  return {
    ...state,
    draft,
    claudeExtra: extra,
    phase: phaseForDraft(state.editor!, draft, extra),
    error: undefined,
    previewing: false,
    previewError: undefined,
    parsing: false,
    parseError: undefined,
  };
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
      const claudeExtra = preserveDraft && prior.draft ? prior.claudeExtra : editor.settings.claudeExtra;
      return { ...current, [target]: { phase: phaseForDraft(editor, draft, claudeExtra), editor, draft, claudeExtra } };
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

function useClientSettingsFragment(busy: boolean, states: States, setStates: SetStates) {
  const revisions = useRef<Record<AppKind, number>>({ codex: 0, claude: 0 });
  const nextRevision = useCallback((app: AppKind) => {
    revisions.current[app] += 1;
    return revisions.current[app];
  }, []);

  const render = useCallback((app: AppKind, draft: Record<string, SettingValue>, extra: ClaudeExtraSettings) => {
    if (busy) return;
    const revision = nextRevision(app);
    setStates((current) => {
      const state = current[app];
      if (!state.draft || !sameValues(state.draft, draft) || !sameClaudeExtra(state.claudeExtra, extra)) return current;
      return { ...current, [app]: { ...state, previewing: true, previewError: undefined } };
    });
    void previewClientSettings(app, clientSettingsPayload(app, draft, extra)).then((preview) => setStates((current) => {
      const state = current[app];
      if (revisions.current[app] !== revision || !state.draft || !sameValues(state.draft, draft) || !sameClaudeExtra(state.claudeExtra, extra)) return current;
      return { ...current, [app]: { ...state, preview, previewing: false } };
    })).catch((caught) => setStates((current) => {
      const state = current[app];
      if (revisions.current[app] !== revision || !state.draft || !sameValues(state.draft, draft) || !sameClaudeExtra(state.claudeExtra, extra)) return current;
      return { ...current, [app]: { ...state, previewing: false, previewError: caught as CommandError } };
    }));
  }, [busy, nextRevision, setStates]);

  const edit = useCallback((app: AppKind, content: string) => {
    if (busy) return;
    const state = states[app];
    if (!state.editor || !state.draft || !state.preview) return;
    const revision = nextRevision(app);
    setStates((current) => {
      const latest = current[app];
      if (!latest.preview) return current;
      return {
        ...current,
        [app]: {
          ...latest,
          preview: { ...latest.preview, content },
          error: undefined,
          previewing: false,
          previewError: undefined,
          parsing: true,
          parseError: undefined,
        },
      };
    });
    void parseClientSettings(app, content).then((parsed) => setStates((current) => {
      const latest = current[app];
      if (
        revisions.current[app] !== revision ||
        !latest.editor ||
        !latest.preview ||
        latest.preview.content !== content
      ) return current;
      return {
        ...current,
        [app]: {
          ...latest,
          draft: parsed.settings,
          claudeExtra: parsed.claudeExtra,
          phase: phaseForDraft(latest.editor, parsed.settings, parsed.claudeExtra),
          parsing: false,
          parseError: undefined,
        },
      };
    })).catch((caught) => setStates((current) => {
      const latest = current[app];
      if (revisions.current[app] !== revision || latest.preview?.content !== content) return current;
      return { ...current, [app]: { ...latest, parsing: false, parseError: caught as CommandError } };
    }));
  }, [busy, nextRevision, setStates, states]);

  return { render, edit };
}

/** Each client preference draft lives in application state until its explicit save. */
export function useClientSettings(deps: ClientSettingsDeps) {
  const { active, app, busy } = deps;
  const [states, setStates] = useState<States>({ codex: { phase: "idle" }, claude: { phase: "idle" } });
  const load = useLoadClientSettings(active, app, states, setStates);
  const fragment = useClientSettingsFragment(busy, states, setStates);
  const previewSettings = useCallback((app: AppKind) => {
    const state = states[app];
    if (busy || state.previewing || !state.draft) return;
    fragment.render(app, state.draft, state.claudeExtra);
  }, [busy, fragment, states]);
  const changeValue = useCallback((app: AppKind, key: string, value: SettingValue) => {
    if (busy) return;
    const state = states[app];
    if (!state.editor || !state.draft) return;
    const draft = { ...state.draft, [key]: value };
    setStates((current) => {
      const latest = current[app];
      if (!latest.editor || !latest.draft) return current;
      return { ...current, [app]: changedState(latest, draft) };
    });
    if (state.preview) fragment.render(app, draft, state.claudeExtra);
  }, [busy, fragment, setStates, states]);
  const retryLoad = useCallback((app: AppKind) => {
    if (!busy) void load(app, states[app].draft !== undefined);
  }, [busy, load, states]);
  const reloadFromStored = useCallback((app: AppKind) => {
    if (!busy) void load(app);
  }, [busy, load]);
  return {
    editorState: states[app],
    changeValue,
    retryLoad,
    reloadFromStored,
    previewSettings,
    changePreviewContent: fragment.edit,
  };
}
