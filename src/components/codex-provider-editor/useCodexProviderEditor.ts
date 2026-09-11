import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { CodexProviderDraft, SettingsValues } from "../../api/client";
import type { CodexEditorSource } from "../../app/useProviders";
import { useProviderConnection } from "../provider-editor/useProviderConnection";
import { useProviderParameters } from "../provider-editor/useProviderParameters";
import { codexDraftFrom, prepareCodexDraft, validateCodexDraft, type CodexEditorDraft } from "./draft";

function codexDraftFromSource(source: CodexEditorSource | null): CodexEditorDraft {
  if (source?.kind === "record") return codexDraftFrom(source.record);
  return codexDraftFrom(null);
}

export function useCodexProviderEditor(source: CodexEditorSource | null, busy: boolean) {
  const [draft, setDraft] = useState<CodexEditorDraft>(() => codexDraftFromSource(source));
  const [parametersOpen, setParametersOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const lastSection = useRef(false);
  // Third-party Codex is always a custom route; Responses is the only upstream
  // that carries request-mode facts.
  const connection = useProviderConnection({
    app: "codex",
    routeMode: "custom",
    baseUrl: draft.endpoint,
    apiKey: draft.apiKey,
    upstreamProtocol: draft.upstream,
    responsesOptions: draft.upstream === "responses" ? { requestMode: draft.requestMode } : null,
  });
  const seedParameters = useCallback((defaults: SettingsValues) => {
    setDraft((current) => current.parameters === null
      ? { ...current, parameters: { settings: { ...defaults.settings } } }
      : current);
  }, []);
  const parameters = useProviderParameters("codex", draft.parameters, seedParameters);
  useEffect(() => {
    if (parametersOpen) headingRef.current?.focus();
    else if (lastSection.current) triggerRef.current?.focus();
    lastSection.current = parametersOpen;
  }, [parametersOpen]);
  const problems = validateCodexDraft(draft);
  const canSave = !busy && parameters.ready && problems.length === 0;
  const save = (onSave: (value: CodexProviderDraft) => void) => {
    if (!canSave) return;
    const value = prepareCodexDraft(draft);
    if (value) onSave(value);
  };
  return { draft, setDraft, parametersOpen, setParametersOpen,
    triggerRef, headingRef, connection, parameters, problems, canSave, save };
}

export type CodexEditorState = ReturnType<typeof useCodexProviderEditor>;

/** Narrow helper for section components that patch one catalog entry. */
export type SetCodexDraft = Dispatch<SetStateAction<CodexEditorDraft>>;
