import { useLayoutEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { AppKind, ConfigFileStatus, LockStatus, RouteState } from "../api/client";
import type { TFunction } from "../i18n";
import { useI18n } from "../i18n";
import { localizedMessageText } from "../i18n/errors";
import { clientName } from "../lib/client-name";
import { currentProviderName, currentProviderProfile, type ActiveProfileRef } from "../lib/current-provider-name";
import { ClientLogo } from "./ClientLogo";
import { ProviderEndpoint } from "./ProviderRowControls";
import "../styles/base/route-cards.css";

interface RouteCardProps {
  app: AppKind;
  status: ConfigFileStatus | undefined;
  profiles: readonly ActiveProfileRef[];
  lock: LockStatus | undefined;
}

function configurationNotes(status: ConfigFileStatus | undefined, t: TFunction): string[] {
  if (!status) return [t("providers.route.notes.notRead")];
  if (status.readError) return [t("providers.route.notes.readError", { detail: status.readError })];
  if (!status.exists) return [t("providers.route.notes.missing")];
  if (!status.syntaxOk) return [t("providers.route.notes.syntaxError")];
  const notes: string[] = [];
  switch (status.matchStatus.kind) {
    case "externallyModified": notes.push(t("providers.route.notes.externallyModified")); break;
    case "profileChanged": notes.push(t("providers.route.notes.profileChanged")); break;
    case "unmanaged": notes.push(t("providers.route.notes.unmanaged")); break;
    case "restoredBackup": notes.push(t("providers.route.notes.restoredBackup")); break;
    case "unknown": notes.push(t("providers.route.notes.unknown")); break;
  }
  return [...notes, ...(status.route?.scopeWarnings ?? []).map((warning) => localizedMessageText(warning, t))];
}

function lockNote(lock: LockStatus | undefined, t: TFunction): string | null {
  if (!lock) return t("providers.route.lock.notRead");
  switch (lock.state) {
    case "free": return null;
    case "stale": return t("providers.route.lock.stale");
    case "held": return t("providers.route.lock.held", {
      name: lock.processName ?? (lock.pid ? t("providers.route.lock.process", { pid: lock.pid }) : t("providers.route.lock.otherProcess")),
    });
    case "indeterminate": return t("providers.route.lock.indeterminate", { detail: lock.reason });
  }
}

function accessLabel(route: RouteState | null, t: TFunction): string {
  if (!route) return t("providers.label.notRead");
  return route.routeMode === "official" ? t("providers.label.officialLogin") : t("providers.route.access.custom");
}

/** Swapping client tabs or views remounts these cards, which would restart
 * the stylesheet's color flow at its canned phase and visibly jump the
 * gradient. Re-anchoring the animation delay to the page clock puts each
 * mount back where the flow would have been; duration and the per-app phase
 * offset stay owned by route-cards.css and are read from computed style. */
function useContinuousFlowPhase() {
  const cardRef = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const card = cardRef.current;
    if (!card) return;
    const styles = getComputedStyle(card);
    // Reduced motion disables the animation, which reads as a 0s duration.
    const durationMs = parseFloat(styles.animationDuration) * 1000;
    if (durationMs <= 0) return;
    const baseDelayMs = parseFloat(styles.animationDelay) * 1000;
    card.style.animationDelay = `${baseDelayMs - (performance.now() % durationMs)}ms`;
  }, []);
  return cardRef;
}

/** Summarizes observed configuration facts without treating valid syntax as health. */
function RouteCard({
  app, status, profiles, lock,
}: RouteCardProps) {
  const { t } = useI18n();
  const cardRef = useContinuousFlowPhase();
  const readable = status && !status.readError && status.exists && status.syntaxOk;
  const route = readable ? status.route : null;
  const activeProfile = currentProviderProfile(status, profiles);
  const modelValue = route ? route.model ?? t("providers.label.clientDefault") : t("providers.label.notRead");
  const serviceValue = serviceAddress(route, t);
  const notes = configurationNotes(status, t);
  const lockWarning = lockNote(lock, t);
  if (lockWarning) notes.push(lockWarning);
  return (
    <section ref={cardRef} className={`asb-route-card${route ? " is-on" : ""}`} data-app={app} aria-label={t("providers.route.cardAria", { client: clientName(app) })}>
      <div className="asb-route-card-body">
        <div>
          <div className="asb-route-ident">
            <ClientLogo app={app} className="asb-route-logo" />
            <span className="asb-route-client">{clientName(app)}</span>
          </div>
          <h3 className="asb-route-provider">{route ? currentProviderName(status, profiles) : t("providers.label.notRead")}</h3>
        </div>
        <dl className="asb-route-values">
          <div><dt className="asb-route-key">{t("providers.label.model")}</dt><dd className="asb-route-value" title={modelValue}>{modelValue}</dd></div>
          <div><dt className="asb-route-key">{t("providers.label.serviceAddress")}</dt><dd className="asb-route-value" title={serviceValue}>{serviceValue}</dd></div>
          <div><dt className="asb-route-key">{t("providers.label.accessMode")}</dt><dd className="asb-route-value">{accessLabel(route, t)}</dd></div>
          <div><dt className="asb-route-key">{t("providers.label.website")}</dt><dd className="asb-route-value">{websiteValue(route, activeProfile?.websiteUrl ?? null, t)}</dd></div>
        </dl>
        {notes.length > 0 && (
          <ul className="asb-route-notes">
            {notes.map((note, index) => <li key={`${index}-${note}`}>{note}</li>)}
          </ul>
        )}
      </div>
    </section>
  );
}

function serviceAddress(route: RouteState | null, t: TFunction): string {
  if (!route) return t("providers.label.notRead");
  if (!route.baseUrl) return route.routeMode === "official" ? t("providers.route.officialService") : t("providers.label.notSet");
  try { return new URL(route.baseUrl).host; } catch { return t("providers.route.unresolvable"); }
}

/** The active profile's homepage as a system-browser link; the route card
 * only states facts read from the real configuration. */
function websiteValue(route: RouteState | null, profileUrl: string | null, t: TFunction): ReactNode {
  if (!route) return t("providers.label.notRead");
  if (!profileUrl) return t("providers.label.notSet");
  return <ProviderEndpoint url={profileUrl} link />;
}

interface DualRelayProps {
  statuses: ConfigFileStatus[] | null;
  profiles: readonly ActiveProfileRef[];
  locks: Partial<Record<AppKind, LockStatus>>;
}

/** Both cards describe observed client files, independently of list selection. */
export function DualRelay({ statuses, profiles, locks }: DualRelayProps) {
  return (
    <>
      <RouteCard app="codex" status={statuses?.find((status) => status.app === "codex")}
        profiles={profiles} lock={locks.codex} />
      <RouteCard app="claude" status={statuses?.find((status) => status.app === "claude")}
        profiles={profiles} lock={locks.claude} />
    </>
  );
}
