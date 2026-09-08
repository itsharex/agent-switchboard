import { useEffect, useRef, useState } from "react";
import type { AppKind, ProviderDraft, ProviderProfile } from "../../api/client";
import { draftFrom, prepareDraft, responsesOptionsValid } from "./draft";
import { useProviderConnection } from "./useProviderConnection";
import { useProviderParameters } from "./useProviderParameters";

export function useProviderEditor(profile: ProviderProfile | null, initialApp: AppKind, busy: boolean) {
  const [draft, setDraft] = useState(() => draftFrom(profile, initialApp));
  const [parametersOpen, setParametersOpen] = useState(false);
  const [loginDone, setLoginDone] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const lastSection = useRef(false);
  const connection = useProviderConnection(draft);
  const parameters = useProviderParameters(draft, setDraft);
  useEffect(() => {
    if (parametersOpen) headingRef.current?.focus();
    else if (lastSection.current) triggerRef.current?.focus();
    lastSection.current = parametersOpen;
  }, [parametersOpen]);
  const canSave = !busy && parameters.ready && responsesOptionsValid(draft) &&
    (draft.routeMode !== "official" || Boolean(profile) || loginDone);
  const save = (onSave: (value: ProviderDraft) => void) => {
    if (!canSave) return;
    const value = prepareDraft(draft);
    if (value) onSave(value);
  };
  return { draft, setDraft, parametersOpen, setParametersOpen, loginDone, setLoginDone,
    triggerRef, headingRef, connection, parameters, canSave, save };
}

export type ProviderEditorState = ReturnType<typeof useProviderEditor>;
