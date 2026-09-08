import { useCallback, useEffect, useRef, useState } from "react";
import {
  applyCodexSubagentSettings,
  getCodexSubagentSettings,
  previewCodexSubagentSettings,
  type CodexSubagentSettings,
  type CodexSubagentSettingsPreview,
  type CodexSubagentSettingsSnapshot,
  type CommandError,
  type SettingValue,
} from "../api/client";

export type CodexSubagentSettingsPhase =
  | "idle"
  | "loading"
  | "loadError"
  | "clean"
  | "dirty"
  | "applying"
  | "applyError";

export interface CodexSubagentSettingsEditorState {
  phase: CodexSubagentSettingsPhase;
  snapshot?: CodexSubagentSettingsSnapshot;
  draft?: CodexSubagentSettings;
  error?: CommandError;
  preview?: CodexSubagentSettingsPreview;
  previewing?: boolean;
  previewError?: CommandError;
}

interface CodexSubagentSettingsDeps {
  active: boolean;
  busy: boolean;
  onError: (error: CommandError) => void;
  clearError: () => void;
  setBusy: (busy: boolean) => void;
  invalidateSwitchCandidates: () => void;
  refresh: () => Promise<void>;
}

type SubagentField = keyof CodexSubagentSettings;

function sameValue(left: SettingValue, right: SettingValue): boolean {
  return left.mode === right.mode && (
    left.mode === "automatic" ||
    (right.mode === "explicit" && left.value === right.value)
  );
}

function sameSettings(left: CodexSubagentSettings, right: CodexSubagentSettings): boolean {
  return (
    sameValue(left.enabled, right.enabled) &&
    sameValue(left.maxConcurrentThreadsPerSession, right.maxConcurrentThreadsPerSession) &&
    sameValue(left.interruptMessage, right.interruptMessage)
  );
}

function phaseForDraft(
  snapshot: CodexSubagentSettingsSnapshot,
  draft: CodexSubagentSettings,
): CodexSubagentSettingsPhase {
  return sameSettings(snapshot.settings, draft) ? "clean" : "dirty";
}

function automaticSettings(): CodexSubagentSettings {
  return {
    enabled: { mode: "automatic" },
    maxConcurrentThreadsPerSession: { mode: "automatic" },
    interruptMessage: { mode: "automatic" },
  };
}

function changedState(
  snapshot: CodexSubagentSettingsSnapshot,
  draft: CodexSubagentSettings,
): CodexSubagentSettingsEditorState {
  return {
    phase: phaseForDraft(snapshot, draft),
    snapshot,
    draft,
  };
}

/**
 * Owns the directly-applied Codex `[agents]` runtime controls. Default model
 * and reasoning effort are provider parameters instead. The confirmed write
 * is hash-bound to the exact previewed candidate.
 */
export function useCodexSubagentSettings({
  active,
  busy,
  onError,
  clearError,
  setBusy,
  invalidateSwitchCandidates,
  refresh,
}: CodexSubagentSettingsDeps) {
  const [state, setState] = useState<CodexSubagentSettingsEditorState>({ phase: "idle" });
  const request = useRef<Promise<void> | null>(null);

  const load = useCallback((preserveDraft = false) => {
    if (request.current) return request.current;
    setState((current) => ({ ...current, phase: "loading", error: undefined }));
    const pending = getCodexSubagentSettings()
      .then((snapshot) => {
        setState((current) => {
          const draft = preserveDraft && current.draft ? current.draft : snapshot.settings;
          return changedState(snapshot, draft);
        });
      })
      .catch((caught) => {
        setState((current) => ({
          ...current,
          phase: "loadError",
          error: caught as CommandError,
        }));
      })
      .finally(() => {
        if (request.current === pending) request.current = null;
      });
    request.current = pending;
    return pending;
  }, []);

  useEffect(() => {
    if (active && state.phase === "idle") void load();
  }, [active, load, state.phase]);

  const changeValue = useCallback((field: SubagentField, value: SettingValue) => {
    if (busy) return;
    setState((current) => {
      if (!current.snapshot || !current.draft || current.phase === "applying") return current;
      return changedState(current.snapshot, { ...current.draft, [field]: value });
    });
  }, [busy]);

  const resetToAutomatic = useCallback(() => {
    if (busy) return;
    setState((current) => {
      if (!current.snapshot || !current.draft || current.phase === "applying") return current;
      return changedState(current.snapshot, automaticSettings());
    });
  }, [busy]);

  const preview = useCallback(() => {
    if (busy || state.previewing || !state.snapshot || !state.draft || state.phase === "applying") return;
    const snapshot = state.snapshot;
    const draft = state.draft;
    setState((current) => ({ ...current, previewing: true, previewError: undefined }));
    void previewCodexSubagentSettings(draft, snapshot.configHash)
      .then((candidate) => {
        setState((current) => {
          if (!current.snapshot || !current.draft || current.snapshot.configHash !== snapshot.configHash ||
            !sameSettings(current.draft, draft)) return current;
          return { ...current, preview: candidate, previewing: false };
        });
      })
      .catch((caught) => {
        setState((current) => {
          if (!current.snapshot || !current.draft || current.snapshot.configHash !== snapshot.configHash ||
            !sameSettings(current.draft, draft)) return current;
          return { ...current, previewing: false, previewError: caught as CommandError };
        });
      });
  }, [busy, state]);

  const applyPreview = useCallback(async () => {
    if (busy || !state.snapshot || !state.draft || !state.preview || state.phase === "applying") {
      return false;
    }
    const snapshot = state.snapshot;
    const draft = state.draft;
    const previewed = state.preview;
    setState((current) => ({ ...current, phase: "applying", error: undefined }));
    setBusy(true);
    clearError();
    try {
      const saved = await applyCodexSubagentSettings({
        settings: draft,
        expectedHash: snapshot.configHash,
        expectedTargetExisted: snapshot.fileExists,
        renderedHash: previewed.renderedHash,
      }, true);
      setState(changedState(saved, saved.settings));
      invalidateSwitchCandidates();
      await refresh().catch((caught) => onError(caught as CommandError));
      return true;
    } catch (caught) {
      const error = caught as CommandError;
      setState((current) => ({ ...current, phase: "applyError", error }));
      onError(error);
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, clearError, invalidateSwitchCandidates, onError, refresh, setBusy, state]);

  const retryLoad = useCallback(() => {
    if (!busy) void load(state.draft !== undefined);
  }, [busy, load, state.draft]);

  return {
    editorState: state,
    changeValue,
    resetToAutomatic,
    retryLoad,
    preview,
    applyPreview,
  };
}
