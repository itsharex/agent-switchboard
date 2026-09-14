import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api/claude-accounts";
import { claudeError, type ClaudeOperations } from "./operations";
export function useClaudeLogin(op: ClaudeOperations, onAccounts: (view: api.ClaudeAccountsView) => void) {
  const [login, setLogin] = useState<api.ClaudeLoginView | null>(null);
  const session = useRef<string | null>(null);
  const generation = useRef(0);
  const mounted = useRef(true);
  const { run, setError, changed } = op;
  const completed = useRef({ onAccounts, changed });
  useEffect(() => { completed.current = { onAccounts, changed }; }, [onAccounts, changed]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; generation.current += 1; if (session.current) void api.cancelClaudeAccountLogin(session.current).catch(() => {}); };
  }, []);
  const start = (request: api.ClaudeLoginRequest) => void run(async () => {
    if (session.current) return;
    const version = ++generation.current;
    const next = await api.startClaudeAccountLogin(request, true);
    if (!mounted.current || version !== generation.current) { await api.cancelClaudeAccountLogin(next.sessionId); return; }
    session.current = next.sessionId; setLogin(next);
  });
  const cancel = useCallback(async () => {
    const id = session.current; generation.current += 1;
    if (!id) return;
    try { await api.cancelClaudeAccountLogin(id); session.current = null; setLogin(null); }
    catch (cause) { setError(claudeError(cause)); }
  }, [setError]);
  useEffect(() => {
    if (!login || login.phase !== "pending") return;
    const version = generation.current;
    let active = true;
    const timer = window.setTimeout(() => {
      void api.pollClaudeAccountLogin(login.sessionId).then(async (next) => {
        if (!active || version !== generation.current) return;
        if (next.phase === "completed") {
          session.current = null; setLogin(null);
          try {
            const accounts = await api.getClaudeAccounts();
            if (mounted.current && version === generation.current) { completed.current.onAccounts(accounts); completed.current.changed("Claude 托管账号已保存，原生 CLI 凭据未改动。"); }
          } catch (cause) {
            if (mounted.current && version === generation.current) setError("登录已完成，但账号列表刷新失败，请重新读取：" + claudeError(cause));
          }
        } else setLogin(next);
      }).catch((cause: unknown) => {
        if (!active || version !== generation.current) return;
        session.current = null; setLogin(null); setError(claudeError(cause));
        void api.cancelClaudeAccountLogin(login.sessionId).catch(() => {});
      });
    }, Math.max(1, login.intervalSeconds) * 1000);
    return () => { active = false; window.clearTimeout(timer); };
  }, [login, setError]);
  return { login, start, cancel };
}
