import { useCallback, useEffect, useState } from "react";
import { getProviderParametersCatalog, type AppKind, type ProviderParametersCatalog, type SettingsValues } from "../../api/client";

/** Loads one client's provider-parameter catalog and seeds the draft exactly
 * once; the seed callback owns any staleness guard for its draft shape. */
export function useProviderParameters(
  app: AppKind,
  parameters: SettingsValues | null,
  seed: (defaults: SettingsValues) => void,
) {
  const [catalog, setCatalog] = useState<ProviderParametersCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    let active = true;
    setCatalog(null);
    setError(null);
    void (async () => {
      try {
        const loaded = await getProviderParametersCatalog(app);
        if (!active) return;
        setCatalog(loaded);
        if (parameters === null) seed({ settings: { ...loaded.defaults.settings } });
      } catch (caught) {
        if (active) setError((caught as { message?: string }).message ?? "参数目录不可用");
      }
    })();
    return () => { active = false; };
    // `parameters`/`seed` are excluded: seeding happens once per catalog load,
    // and the seed callback owns its own staleness guard.
  }, [app, attempt]);

  const currentCatalog = catalog?.app === app ? catalog : null;
  return { catalog: currentCatalog, error, retry, ready: currentCatalog !== null && parameters !== null };
}
