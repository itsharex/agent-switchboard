import { useMessageState } from "../i18n/use-message-state";
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
import { useI18n } from "@/i18n";
import { TrayProviderItem } from "./TrayProviderItem";
import { useTraySnapshot } from "./useTraySnapshot";

/** Hard cap for the native popup height. The single-line row density keeps
 * realistic provider counts fully visible below it; an extreme count scrolls
 * inside .tray-list as the physical last resort instead of stretching. */
const TRAY_MAX_HEIGHT = 560;

type TrayApp = "codex" | "claude";

/** In-flight tray action. A provider switch carries the row id so that row
 * can show the switching state; panel commands (open/manage/quit) share one
 * shape. The two kinds never share an id namespace. */
type PendingAction = { kind: "switch"; providerId: string } | { kind: "panel" };

interface TrayGroupProps {
  app: TrayApp;
  providers: TraySnapshot["providers"];
  loaded: boolean;
  busy: boolean;
  switchingId: string | null;
  onSwitch: (providerId: string) => void;
}

function TrayGroup({ app, providers, loaded, busy, switchingId, onSwitch }: TrayGroupProps) {
  const label = app === "codex" ? "Codex" : "Claude Code";
  const { t } = useI18n();
  return (
    <section aria-label={label} className="tray-group">
      <h2 className="tray-group-heading"><ClientLogo app={app} className="tray-client-logo" />{label}</h2>
      {loaded && providers.length === 0 && <p className="tray-group-empty">{t("tray.emptyGroup")}</p>}
      {providers.map((provider) => (
        <TrayProviderItem
          key={provider.id}
          provider={provider}
          busy={busy}
          switchingId={switchingId}
          onSwitch={() => onSwitch(provider.id)}
        />
      ))}
    </section>
  );
}

function TrayPanelContent({ snapshot, readError, initialized, refresh }: {
  snapshot: TraySnapshot | null;
  readError: string | null;
  initialized: boolean;
  refresh: () => Promise<void>;
}) {
  const { t } = useI18n();
  const [actionError, setActionError] = useMessageState();
  const [pending, setPending] = useState<PendingAction | null>(null);
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
        void hideTray().catch((caught) => { if (mounted.current) setActionError(caught); });
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
          if (!disposed && request === revision) setActionError(caught);
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
            catch (caught) { if (mounted.current) setActionError(caught); }
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

  const run = async (mark: PendingAction, action: () => Promise<void>) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    setPending(mark);
    setActionError(null);
    try { await action(); }
    catch (caught) { if (mounted.current) setActionError(caught); }
    finally {
      actionInFlight.current = false;
      if (mounted.current) setPending(null);
    }
  };
  const error = actionError ?? readError ?? snapshot?.error;
  const providers = snapshot?.providers ?? [];
  const switchingId = pending?.kind === "switch" ? pending.providerId : null;
  const switchProvider = (providerId: string) => {
    void run({ kind: "switch", providerId }, async () => {
      await switchTrayProvider(providerId);
      await refresh();
    });
  };
  return (
    <div ref={panel} className="tray-panel" aria-label={t("tray.panel.aria")}>
      <header className="tray-header">
        <img src={appIcon} alt="" className="tray-app-logo" aria-hidden="true" />
        <span className="tray-app-name">Agent Switchboard</span>
      </header>
      {error && <div role="alert" className="tray-error">{error}</div>}
      <div ref={list} className="tray-list">
        <div>
          {!snapshot && !readError && <p role="status" className="tray-loading">{t("tray.loading")}</p>}
          {(["codex", "claude"] as const).map((app) => (
            <TrayGroup
              key={app}
              app={app}
              providers={providers.filter((provider) => provider.app === app)}
              loaded={snapshot !== null}
              busy={busy}
              switchingId={switchingId}
              onSwitch={switchProvider}
            />
          ))}
        </div>
      </div>
      <footer className="tray-footer">
        <Button variant="unstyled" className="tray-ghost-button" disabled={busy} onClick={() => void run({ kind: "panel" }, openTrayMain)}>
          <ArrowUpRight size={16} aria-hidden="true" />
          {t("tray.openMain")}
        </Button>
        <Button variant="unstyled" className="tray-ghost-button" disabled={busy} onClick={() => void run({ kind: "panel" }, quitTray)}><LogOut size={16} aria-hidden="true" />{t("tray.quit")}</Button>
      </footer>
    </div>
  );
}

/** The snapshot is accepted before the saved language preference reaches
 * the panel before its first translated render. */
export function TrayPanel() {
  const { snapshot, error: readError, initialized, refresh } = useTraySnapshot();
  return (
    <>
      <TrayPanelContent snapshot={snapshot} readError={readError} initialized={initialized} refresh={refresh} />
    </>
  );
}
