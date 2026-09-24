import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderDraft, ProviderProfile, SettingsValues } from "../../api/client";
import { tr } from "../../i18n/current";
import { draftFrom, prepareDraft, type ProviderEditorDraft } from "../provider-editor/draft";
import { useProviderParameters } from "../provider-editor/useProviderParameters";

function officialDraft(profile: ProviderProfile | null): ProviderEditorDraft {
  if (profile) return draftFrom(profile, "codex");
  return {
    app: "codex", routeMode: "official", name: tr("codex.official.defaultName"),
    baseUrl: null, apiKey: "", upstreamProtocol: null, responsesOptions: null,
    maxOutputTokens: null, model: null, modelOptions: null, parameters: null,
    notes: null, websiteUrl: null, officialQuotaRefreshIntervalMinutes: null,
  };
}

/** Official records retain their saved parameters; only new drafts receive defaults. */
export function useCodexOfficialEditor(profile: ProviderProfile | null, busy: boolean) {
  const [draft, setDraft] = useState(() => officialDraft(profile));
  const [parametersOpen, setParametersOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const lastSection = useRef(false);
  const seed = useCallback((defaults: SettingsValues) => {
    setDraft((current) => current.parameters === null
      ? { ...current, parameters: { settings: { ...defaults.settings } } }
      : current);
  }, []);
  const parameters = useProviderParameters("codex", draft.parameters, seed);
  useEffect(() => {
    if (parametersOpen) headingRef.current?.focus();
    else if (lastSection.current) triggerRef.current?.focus();
    lastSection.current = parametersOpen;
  }, [parametersOpen]);
  const canSave = !busy && parameters.ready && Boolean(draft.name.trim());
  const save = (onSave: (value: ProviderDraft) => void) => {
    if (!canSave) return;
    const value = prepareDraft(draft);
    if (value) onSave(value);
  };
  return { draft, setDraft, parameters, parametersOpen, setParametersOpen,
    triggerRef, headingRef, canSave, save };
}
