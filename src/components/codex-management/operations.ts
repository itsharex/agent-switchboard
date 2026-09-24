import { useMessageState } from "../../i18n/use-message-state";
import { useCallback, useRef, useState } from "react";

export function useCodexOperations(onChanged: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useMessageState();
  const [notice, setNotice] = useMessageState();
  const running = useRef(false);
  const run = useCallback(async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
    if (running.current) return undefined;
    running.current = true; setBusy(true); setError(null); setNotice(null);
    try { return await action(); }
    catch (cause) { setError(cause); return undefined; }
    finally { running.current = false; setBusy(false); }
  }, []);
  const changed = useCallback((message: unknown) => { setNotice(message); onChanged(); }, [onChanged]);
  return { busy, error, notice, run, changed, setError };
}
export type CodexOperations = ReturnType<typeof useCodexOperations>;
