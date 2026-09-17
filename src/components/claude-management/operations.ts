import { useCallback, useRef, useState } from "react";

export function useClaudeOperations(onChanged: () => void) {
  const running = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const run = useCallback(async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
    if (running.current) return undefined;
    running.current = true; setBusy(true); setError(null); setNotice(null);
    try { return await action(); }
    catch (cause) { setError(cause && typeof cause === "object" && "message" in cause && typeof cause.message === "string" ? cause.message : String(cause)); return undefined; }
    finally { running.current = false; setBusy(false); }
  }, []);
  const changed = useCallback((message: string) => { setNotice(message); onChanged(); }, [onChanged]);
  return { run, busy, error, notice, setError, changed };
}
export type ClaudeOperations = ReturnType<typeof useClaudeOperations>;
