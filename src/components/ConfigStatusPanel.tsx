import type { ReactNode } from "react";
import { openConfigFileLocation, type AppKind, type ConfigFileStatus, type LockStatus, type MatchStatus, type ProviderProfile } from "../api/client";
import { clientName } from "../lib/client-name";
import { currentProviderName } from "../lib/current-provider-name";
import { useI18n, type TFunction } from "../i18n";
import { localizedMessageText } from "../i18n/errors";
import { Button } from "./Button";
import { ClientLogo } from "./ClientLogo";
import { FactPath } from "./FactPath";
import { Time } from "./Time";
import { ModuleHeader } from "./WorkspaceHeader";

export interface ConfigStatusPanelProps {
  statuses: ConfigFileStatus[] | null;
  profiles: ProviderProfile[];
  locks: Partial<Record<AppKind, LockStatus>>;
  busy: boolean;
  onRecoverLock: (app: AppKind) => void;
}

function matchLabel(t: TFunction, status: MatchStatus): ReactNode {
  switch (status.kind) {
    case "matchesProfile":
      return t("clientConfig.match.matchesProfile", { name: status.profileName });
    case "profileChanged":
      return t("clientConfig.match.profileChanged", { name: status.profileName });
    case "restoredBackup":
      return <>{t("clientConfig.match.restoredBefore")}<Time iso={status.at} />{t("clientConfig.match.restoredAfter")}</>;
    case "externallyModified":
      return <>{t("clientConfig.match.externalBefore")}<Time iso={status.at} />{t("clientConfig.match.externalAfter")}</>;
    case "unmanaged":
      return t("clientConfig.match.unmanaged");
    case "unknown":
      return t("clientConfig.match.unknown");
  }
}

function lockLabel(t: TFunction, status: LockStatus | undefined): string {
  if (!status) return t("clientConfig.lock.loading");
  switch (status.state) {
    case "free":
      return t("clientConfig.lock.free");
    case "held": {
      const holder = status.processName ?? (status.pid ? t("clientConfig.lock.process", { pid: status.pid }) : t("clientConfig.lock.otherProcess"));
      return t("clientConfig.lock.held", { holder });
    }
    case "stale":
      return t("clientConfig.lock.stale");
    case "indeterminate":
      return t("clientConfig.lock.indeterminate", { reason: status.reason });
  }
}

function statusPill(t: TFunction, status: ConfigFileStatus): { ok: boolean; text: string } {
  if (status.recoveryIssue) return { ok: false, text: t("clientConfig.pill.recoveryPending") };
  if (status.readError) return { ok: false, text: t("clientConfig.pill.readFailed") };
  if (!status.exists) return { ok: false, text: t("clientConfig.pill.missing") };
  if (!status.syntaxOk) return { ok: false, text: t("clientConfig.pill.syntaxError") };
  return { ok: true, text: t("clientConfig.pill.ok") };
}

function StatusField({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return <div><dt>{label}</dt><dd className={className}>{children}</dd></div>;
}

function ConfigStatusDetails({ status, profiles, lock }: {
  status: ConfigFileStatus;
  profiles: ProviderProfile[];
  lock: LockStatus | undefined;
}) {
  const { t } = useI18n();
  const readable = !status.readError && status.exists && status.syntaxOk;
  return (
    <dl className="asb-fact-row">
      <StatusField label={t("clientConfig.details.configFile")}>
        <FactPath path={status.path} open={() => openConfigFileLocation(status.app)} />
      </StatusField>
      {status.recoveryIssue && (
        <StatusField label={t("clientConfig.details.recoveryBlocked")} className="asb-warn-text">
          <span role="status" className="asb-status-warn">{status.recoveryIssue}</span>
          <span className="asb-status-warn">{t("clientConfig.details.recoveryHelp")}</span>
        </StatusField>
      )}
      {status.readError && <StatusField label={t("clientConfig.details.readError")} className="asb-warn-text">{status.readError}</StatusField>}
      {readable && <>
        <StatusField label={t("clientConfig.details.currentProvider")}>{currentProviderName(status, profiles)} · {status.route?.model ?? t("clientConfig.details.defaultModel")}</StatusField>
        <StatusField label={t("clientConfig.details.matchStatus")}>{matchLabel(t, status.matchStatus)}</StatusField>
      </>}
      {status.lastSwitch && (
        <StatusField label={t("clientConfig.details.lastWrite")}>
          <Time iso={status.lastSwitch.at} />
          {status.lastSwitch.operation === "restore"
            ? t("clientConfig.details.lastRestore")
            : status.lastSwitch.operation === "gatewayPortChange"
              ? t("clientConfig.details.lastGatewayPort")
              : status.lastSwitch.profileName
                ? t("clientConfig.details.lastProjected", { name: status.lastSwitch.profileName })
                : t("clientConfig.details.lastWritten")}
        </StatusField>
      )}
      {(status.route?.scopeWarnings.length ?? 0) > 0 && (
        <StatusField label={t("clientConfig.details.scopeWarnings")}>
          {status.route?.scopeWarnings.map((warning) => <span key={warning.key} className="asb-warn-text asb-status-warn">{localizedMessageText(warning, t)}</span>)}
        </StatusField>
      )}
      <StatusField label={t("clientConfig.details.writeLock")}>{lockLabel(t, lock)}</StatusField>
    </dl>
  );
}

function ConfigStatusCard({ status, profiles, lock, busy, onRecoverLock }: {
  status: ConfigFileStatus;
  profiles: ProviderProfile[];
  lock: LockStatus | undefined;
  busy: boolean;
  onRecoverLock: (app: AppKind) => void;
}) {
  const { t } = useI18n();
  const pill = statusPill(t, status);
  return (
    <article className="asb-client-status" aria-label={t("clientConfig.details.statusAria", { name: clientName(status.app) })}>
      <header className="asb-client-status-head">
        <span className="asb-client-status-name"><ClientLogo app={status.app} className="asb-status-logo" />{clientName(status.app)}</span>
        <span className={`asb-status-pill${pill.ok ? " is-ok" : ""}`}>
          <span className="asb-status-pill-dot" aria-hidden="true" />{pill.text}
        </span>
      </header>
      <ConfigStatusDetails status={status} profiles={profiles} lock={lock} />
      {lock?.state === "stale" && (
        <div className="asb-kv-actions">
          <Button variant="secondary" disabled={busy} onClick={() => onRecoverLock(status.app)}>{t("clientConfig.details.recoverLock")}</Button>
        </div>
      )}
    </article>
  );
}

export function ConfigStatusPanel({ statuses, profiles, locks, busy, onRecoverLock }: ConfigStatusPanelProps) {
  const { t } = useI18n();
  return (
    <section className="asb-panel" aria-labelledby="configuration-status-heading">
      <ModuleHeader id="configuration-status-heading" title={t("clientConfig.statusPanel.title")} />
      {statuses === null ? (
        /* Two client sections at the real anatomy's footprint: a head line and
           five fact lines each, so the panel does not reflow on arrival. */
        <div role="status" aria-label={t("clientConfig.statusPanel.loading")}>
          {Array.from({ length: 2 }, (_, section) => (
            <div key={section} className="asb-client-status asb-client-status-skeleton" aria-hidden="true">
              {Array.from({ length: 6 }, (_, line) => (
                <span key={line} className="asb-skeleton" />
              ))}
            </div>
          ))}
        </div>
      ) : (
        statuses.map((status) => (
          <ConfigStatusCard key={status.app} status={status} profiles={profiles} lock={locks[status.app]}
            busy={busy} onRecoverLock={onRecoverLock} />
        ))
      )}
    </section>
  );
}
