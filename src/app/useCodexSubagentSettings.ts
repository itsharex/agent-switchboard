import { useCallback, useEffect, useRef, useState } from "react";
import {
  getCodexSubagentSettings,
  onClientConfigChanged,
  type CodexSubagentSettings,
  type CodexSubagentSettingsSnapshot,
  type CommandError,
  type SettingValue,
} from "../api/client";

export type CodexSubagentSettingsPhase = "idle" | "loading" | "loadError" | "clean" | "dirty";

export interface CodexSubagentSettingsEditorState {
  phase: CodexSubagentSettingsPhase;
  snapshot?: CodexSubagentSettingsSnapshot;
  draft?: CodexSubagentSettings;
  error?: CommandError;
}

interface CodexSubagentSettingsDeps {
  active: boolean;
  busy: boolean;
  onError: (error: CommandError) => void;
}

type SubagentField = keyof CodexSubagentSettings;

function sameValue(left: SettingValue, right: SettingValue): boolean {
  return left.mode === right.mode && (left.mode === "automatic" ||
    (right.mode === "explicit" && left.value === right.value));
}

function sameSettings(left: CodexSubagentSettings, right: CodexSubagentSettings): boolean {
  return sameValue(left.enabled, right.enabled) &&
    sameValue(left.maxConcurrentThreadsPerSession, right.maxConcurrentThreadsPerSession) &&
    sameValue(left.interruptMessage, right.interruptMessage);
}

function stateFor(snapshot: CodexSubagentSettingsSnapshot, draft: CodexSubagentSettings): CodexSubagentSettingsEditorState {
  return { phase: sameSettings(snapshot.settings, draft) ? "clean" : "dirty", snapshot, draft };
}

export function useCodexSubagentSettings({ active, busy, onError }: CodexSubagentSettingsDeps) {
  const [state, setState] = useState<CodexSubagentSettingsEditorState>({ phase: "idle" });
  const request = useRef<Promise<void> | null>(null);
  const draft = useRef<CodexSubagentSettings | undefined>(undefined);
  const wasActive = useRef(false);
  draft.current = state.draft;
  const load = useCallback((preserveDraft = false) => {
    if (request.current) return request.current;
    setState((current) => ({ ...current, phase: "loading", error: undefined }));
    const pending = getCodexSubagentSettings().then((snapshot) => setState((current) =>
      stateFor(snapshot, preserveDraft && current.draft ? current.draft : snapshot.settings),
    )).catch((caught) => setState((current) => ({ ...current, phase: "loadError", error: caught as CommandError })))
      .finally(() => { if (request.current === pending) request.current = null; });
    request.current = pending;
    return pending;
  }, []);
  useEffect(() => {
    const entering = active && !wasActive.current;
    wasActive.current = active;
    if (entering) void load(draft.current !== undefined);
  }, [active, load]);
  useEffect(() => {
    let disposed = false; let unlisten: (() => void) | undefined;
    void onClientConfigChanged(() => { if (active && !busy) void load(true); }).then((stop) => {
      if (disposed) stop(); else unlisten = stop;
    }).catch((caught) => { if (!disposed) onError(caught as CommandError); });
    return () => { disposed = true; unlisten?.(); };
  }, [active, busy, load, onError]);
  const changeValue = useCallback((field: SubagentField, value: SettingValue) => {
    if (busy) return;
    setState((current) => current.snapshot && current.draft
      ? stateFor(current.snapshot, { ...current.draft, [field]: value }) : current);
  }, [busy]);
  const retryLoad = useCallback(() => { if (!busy) void load(state.draft !== undefined); }, [busy, load, state.draft]);
  const reloadFromFile = useCallback(() => { if (!busy) void load(); }, [busy, load]);
  return { editorState: state, changeValue, retryLoad, reloadFromFile };
}
