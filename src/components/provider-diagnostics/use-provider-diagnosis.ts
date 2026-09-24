import { useCallback, useEffect, useRef, useState } from "react";
import { diagnoseProvider, type ProviderDiagnosticsReport } from "../../api/provider-diagnostics";
import { useMessageState } from "../../i18n/use-message-state";

export function useProviderDiagnosis(profileId: string) {
  const [report, setReport] = useState<ProviderDiagnosticsReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useMessageState();
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++generation.current;
    setBusy(true); setReport(null); setError(null);
    try {
      const next = await diagnoseProvider(profileId);
      if (generation.current === version) setReport(next);
    } catch (caught) {
      if (generation.current === version) setError(caught);
    } finally {
      if (generation.current === version) setBusy(false);
    }
  }, [profileId, setError]);
  useEffect(() => {
    void refresh();
    return () => { generation.current += 1; };
  }, [refresh]);
  return { report, busy, error, refresh };
}
