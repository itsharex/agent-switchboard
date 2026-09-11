import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowUpRight, Check, LogOut } from "lucide-react";
import { hideTray, openTrayMain, quitTray, resizeTray, switchTrayProvider, trayReady } from "../api/client";
import appIcon from "../assets/app-icon.png";
import { Button } from "@/components/Button";
import { ClientLogo } from "@/components/ClientLogo";
import { Time } from "@/components/Time";
import { formatUsageSummary } from "@/lib/usage-format";
import { applyAppAppearance } from "@/lib/app-appearance";
import { cx } from "@/utils/cx";
import { trayError, useTraySnapshot } from "./useTraySnapshot";

export function TrayPanel() {
  const { snapshot, error: readError, initialized, refresh } = useTraySnapshot();
  const [actionError, setActionError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const actionInFlight = useRef(false);
  const readySent = useRef(false);
  const mounted = useRef(true);
  const panel = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const busy = pending !== null || snapshot?.switching === true;

  useEffect(() => {
    mounted.current = true;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void hideTray().catch((caught) => { if (mounted.current) setActionError(trayError(caught)); });
      }
    };
    document.addEventListener("keydown", onKey);
    return () => { mounted.current = false; document.removeEventListener("keydown", onKey); };
  }, []);

  useEffect(() => {
    applyAppAppearance(snapshot?.settings ?? null);
  }, [snapshot?.settings]);

  useLayoutEffect(() => {
    let disposed = false;
    let revision = 0;
    let lastHeight = 0;
    const measure = () => {
      if (!panel.current || !list.current) return;
      const height = Math.ceil(panel.current.offsetHeight - list.current.clientHeight + list.current.scrollHeight);
      if (height <= 0 || height === lastHeight) return;
      lastHeight = height;
      const request = ++revision;
      void (async () => {
        try { await resizeTray(height); }
        catch (caught) {
          if (!disposed && request === revision) setActionError(trayError(caught));
        } finally {
          // A failed resize must still reveal the recovery controls. Only the
          // latest committed layout may complete the initial native handshake.
          if (!disposed && request === revision && initialized && !readySent.current) {
            readySent.current = true;
            try { await trayReady(); }
            catch (caught) { if (mounted.current) setActionError(trayError(caught)); }
          }
        }
      })();
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (panel.current) observer.observe(panel.current);
    if (list.current?.firstElementChild) observer.observe(list.current.firstElementChild);
    return () => { disposed = true; observer.disconnect(); };
  }, [snapshot, readError, actionError, initialized]);

  const act = async (id: string, action: () => Promise<void>) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    setPending(id);
    setActionError(null);
    try { await action(); }
    catch (caught) { if (mounted.current) setActionError(trayError(caught)); }
    finally {
      actionInFlight.current = false;
      if (mounted.current) setPending(null);
    }
  };
  const error = actionError ?? readError ?? snapshot?.error;
  return (
    <div ref={panel} className="tray-panel" aria-label="Agent Switchboard 托盘">
      <header className="tray-header">
        <img src={appIcon} alt="" className="tray-app-logo" aria-hidden="true" />
        <Button variant="secondary" className="tray-ghost-button" disabled={busy} onClick={() => void act("open", () => openTrayMain(false))}>
          <ArrowUpRight size={16} aria-hidden="true" />
          打开主界面
        </Button>
      </header>
      {error && <div role="alert" className="tray-error">{error}</div>}
      <div ref={list} className="tray-list">
        <div>
          {!snapshot && !readError && <p role="status" className="tray-loading">正在读取供应商…</p>}
          {(["codex", "claude"] as const).map((app) => {
            const providers = snapshot?.providers.filter((provider) => provider.app === app) ?? [];
            return <section key={app} aria-label={app === "codex" ? "Codex" : "Claude Code"} className="tray-group">
              <h2 className="tray-group-heading"><ClientLogo app={app} className="tray-client-logo" />{app === "codex" ? "Codex" : "Claude Code"}</h2>
              {snapshot && providers.length === 0 && <p className="tray-group-empty">暂无供应商</p>}
              {providers.map((provider) => <Button
                key={provider.id} variant="secondary"
                className={cx("tray-provider", provider.active && "tray-provider-active")}
                disabled={busy || provider.active}
                aria-label={provider.active ? `${provider.name}，当前供应商` : `切换到 ${provider.name}`}
                aria-describedby={`tray-provider-detail-${provider.id}`}
                onClick={() => void act(provider.id, async () => { await switchTrayProvider(provider.id); await refresh(); })}
              >
                <span className="tray-check">{provider.active && <Check size={18} aria-hidden="true" />}</span>
                <span className="tray-provider-text">
                  <span className="tray-provider-name">{provider.name}</span>
                  <span id={`tray-provider-detail-${provider.id}`} className="tray-provider-text">
                  <span className="tray-detail">{provider.model ?? "默认模型"}</span>
                  {provider.usage && <>
                    <span className="tray-detail">{formatUsageSummary(provider.usage)}</span>
                    <span className="tray-time">缓存 · <Time iso={provider.usage.at} /></span>
                  </>}
                  </span>
                </span>
                {!provider.active && <span className="tray-switch">{pending === provider.id ? "切换中" : "切换"}</span>}
              </Button>)}
            </section>;
          })}
        </div>
      </div>
      <footer className="tray-footer">
        <Button variant="secondary" className="tray-ghost-button" disabled={busy} onClick={() => void act("manage", () => openTrayMain(true))}>管理供应商</Button>
        <Button variant="secondary" className="tray-ghost-button" disabled={busy} onClick={() => void act("quit", quitTray)}><LogOut size={16} aria-hidden="true" />退出</Button>
      </footer>
    </div>
  );
}
