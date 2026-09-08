import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { getProviderParametersCatalog, type ProviderParametersCatalog } from "../../api/client";
import type { ProviderEditorDraft } from "./draft";

export function useProviderParameters(
  draft: ProviderEditorDraft,
  setDraft: Dispatch<SetStateAction<ProviderEditorDraft>>,
) {
  const [catalog, setCatalog] = useState<ProviderParametersCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);
  const app = draft.app;

  useEffect(() => {
    let active = true;
    setCatalog(null);
    setError(null);
    void (async () => {
      try {
        const loaded = await getProviderParametersCatalog(app);
        if (!active) return;
        setCatalog(loaded);
        setDraft((current) => current.app === app && current.parameters === null
          ? { ...current, parameters: { settings: { ...loaded.defaults.settings } } }
          : current);
      } catch (caught) {
        if (active) setError((caught as { message?: string }).message ?? "参数目录不可用");
      }
    })();
    return () => { active = false; };
  }, [app, attempt, setDraft]);

  const currentCatalog = catalog?.app === app ? catalog : null;
  return { catalog: currentCatalog, error, retry, ready: currentCatalog !== null && draft.parameters !== null };
}
