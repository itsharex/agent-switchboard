import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowUpRight, LogOut } from "lucide-react";
import {
  hideTray,
  openTrayMain,
  quitTray,
  resizeTray,
  switchTrayProvider,
  trayReady,
  type TraySnapshot,
} from "../api/client";
import appIcon from "../assets/app-icon.svg";
import { Button } from "@/components/Button";
import { ClientLogo } from "@/components/ClientLogo";
import { applyAppAppearance } from "@/lib/app-appearance";
import { TrayProviderItem } from "./TrayProviderItem";
import { trayError, useTraySnapshot } from "./useTraySnapshot";

/** Hard cap for the native popup height. The single-line row density keeps
 * realistic provider counts fully visible below it; an extreme count scrolls
 * inside .tray-list as the physical last resort instead of stretching. */
const TRAY_MAX_HEIGHT = 560;

type TrayApp = "codex" | "claude";

interface TrayGroupProps {
  app: TrayApp;
  providers: TraySnapshot["providers"];
  loaded: boolean;
  busy: boolean;
  pending: string | null;
  onSwitch: (providerId: string) => void;
}

function TrayGroup({ app, providers, loaded, busy, pending, onSwitch }: TrayGroupProps) {
  const label = app === "codex" ? "Codex" : "Claude Code";
  return (
    <section aria-label={label} className="tray-group">
      <h2 className="tray-group-heading"><ClientLogo app={app} className="tray-client-logo" />{label}</h2>
      {loaded && providers.length === 0 && <p className="tray-group-empty">暂无供应商</p>}
      {providers.map((provider) => (
        <TrayProviderItem
          key={provider.id}
          provider={provider}
          busy={busy}
          pending={pending}
          onSwitch={() => onSwitch(provider.id)}
        />
      ))}
    </section>
  );
}

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
      const content = Math.ceil(panel.current.offsetHeight - list.current.clientHeight + list.current.scrollHeight);
      const height = Math.min(content, TRAY_MAX_HEIGHT);
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
            try {
              await trayReady();
              // Marking sent only after success keeps a one-off handshake
              // failure retryable; marking it before the await would leave
              // the native side never-ready and the panel could never open.
              readySent.current = true;
            }
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
  const providers = snapshot?.providers ?? [];
  const switchProvider = (providerId: string) => {
    void act(providerId, async () => {
      await switchTrayProvider(providerId);
      await refresh();
    });
  };
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
          {(["codex", "claude"] as const).map((app) => (
            <TrayGroup
              key={app}
              app={app}
              providers={providers.filter((provider) => provider.app === app)}
              loaded={snapshot !== null}
              busy={busy}
              pending={pending}
              onSwitch={switchProvider}
            />
          ))}
        </div>
      </div>
      <footer className="tray-footer">
        <Button variant="secondary" className="tray-ghost-button" disabled={busy} onClick={() => void act("manage", () => openTrayMain(true))}>管理供应商</Button>
        <Button variant="secondary" className="tray-ghost-button" disabled={busy} onClick={() => void act("quit", quitTray)}><LogOut size={16} aria-hidden="true" />退出</Button>
      </footer>
    </div>
  );
}
