import { useCallback, useEffect, useRef, useState } from "react";
import { cancelCodexAccountLogin, pollCodexAccountLogin, startCodexAccountLogin, type CodexAccountsView, type CodexAccountLoginStart } from "../../api/codex-accounts";
import { messageFor, type CodexOperations } from "./operations";
export function useManagedLogin(operations: CodexOperations, onCompleted: (view: CodexAccountsView) => void) {
  const [login, setLogin] = useState<CodexAccountLoginStart | null>(null);
  const generation = useRef(0);
  const callbacks = useRef({ onCompleted, changed: operations.changed, setError: operations.setError });
  callbacks.current = { onCompleted, changed: operations.changed, setError: operations.setError };
  const start = (id: string | null) => void operations.run(async () => { setLogin(await startCodexAccountLogin(id)); });
  const cancel = useCallback(async () => {
    if (!login) return;
    generation.current++; await cancelCodexAccountLogin(login.sessionId);
    setLogin((current) => current?.sessionId === login.sessionId ? null : current);
  }, [login]);
  useEffect(() => {
    if (!login) return;
    const current = ++generation.current; let polling = false; let finished = false;
    const timer = window.setInterval(async () => {
      if (polling) return; polling = true;
      try {
        const result = await pollCodexAccountLogin(login.sessionId);
        if (generation.current !== current) return;
        if (result.phase === "completed") {
          finished = true; setLogin(null); callbacks.current.onCompleted(result.accounts);
          callbacks.current.changed("Codex 账号已保存到本地库，尚未改动客户端登录。");
        } else if (result.phase === "failed") { finished = true; setLogin(null); callbacks.current.setError(result.message); }
      } catch (error) { if (generation.current === current) callbacks.current.setError(messageFor(error)); }
      finally { polling = false; }
    }, 3000);
    return () => {
      window.clearInterval(timer); generation.current++;
      if (!finished) void cancelCodexAccountLogin(login.sessionId).catch(() => {});
    };
  }, [login]);
  return { login, start, cancel };
}
