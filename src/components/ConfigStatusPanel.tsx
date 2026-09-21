import type { ReactNode } from "react";
import { openConfigFileLocation, type AppKind, type ConfigFileStatus, type LockStatus, type MatchStatus, type ProviderProfile } from "../api/client";
import { clientName } from "../lib/client-name";
import { currentProviderName } from "../lib/current-provider-name";
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

function matchLabel(status: MatchStatus): ReactNode {
  switch (status.kind) {
    case "matchesProfile":
      return `与档案「${status.profileName}」一致`;
    case "profileChanged":
      return `档案「${status.profileName}」或客户端设置已变更，尚未应用`;
    case "restoredBackup":
      return <>当前为已恢复备份（<Time iso={status.at} />）</>;
    case "externallyModified":
      return <>与上次切换（<Time iso={status.at} />）不符，配置可能被外部修改</>;
    case "unmanaged":
      return "从未由本应用切换，也不匹配任何档案";
    case "unknown":
      return "无法评估（文件缺失或语法错误）";
  }
}

function lockLabel(status: LockStatus | undefined): string {
  if (!status) return "写入锁状态加载中";
  switch (status.state) {
    case "free":
      return "写入锁空闲";
    case "held": {
      const holder = status.processName ?? (status.pid ? `进程 ${status.pid}` : "其他进程");
      return `写入锁由${holder}持有`;
    }
    case "stale":
      return "发现遗留写入锁，可在确认后清理";
    case "indeterminate":
      return `写入锁状态无法确定：${status.reason}`;
  }
}

function statusPill(status: ConfigFileStatus): { ok: boolean; text: string } {
  if (status.recoveryIssue) return { ok: false, text: "等待配置恢复" };
  if (status.readError) return { ok: false, text: "读取失败" };
  if (!status.exists) return { ok: false, text: "未找到配置文件" };
  if (!status.syntaxOk) return { ok: false, text: "语法错误" };
  return { ok: true, text: "配置正常" };
}

function StatusField({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return <div><dt>{label}</dt><dd className={className}>{children}</dd></div>;
}

function ConfigStatusDetails({ status, profiles, lock }: {
  status: ConfigFileStatus;
  profiles: ProviderProfile[];
  lock: LockStatus | undefined;
}) {
  const readable = !status.readError && status.exists && status.syntaxOk;
  return (
    <dl className="asb-fact-row">
      <StatusField label="配置文件">
        <FactPath path={status.path} open={() => openConfigFileLocation(status.app)} />
      </StatusField>
      {status.recoveryIssue && (
        <StatusField label="恢复阻塞" className="asb-warn-text">
          <span role="status" className="asb-status-warn">{status.recoveryIssue}</span>
          <span className="asb-status-warn">配置写入暂不可用。请前往「设置 → 备份恢复」查看对应备份与差异；恢复后刷新状态。</span>
        </StatusField>
      )}
      {status.readError && <StatusField label="读取错误" className="asb-warn-text">{status.readError}</StatusField>}
      {readable && <>
        <StatusField label="当前服务">{currentProviderName(status, profiles)} · {status.route?.model ?? "默认模型"}</StatusField>
        <StatusField label="匹配状态">{matchLabel(status.matchStatus)}</StatusField>
      </>}
      {status.lastSwitch && (
        <StatusField label="上次写入">
          <Time iso={status.lastSwitch.at} />
          {status.lastSwitch.operation === "restore"
            ? " · 已恢复备份"
            : status.lastSwitch.operation === "gatewayPortChange"
              ? " · 已修改网关监听端口"
              : status.lastSwitch.profileName
                ? ` · 已投影供应商「${status.lastSwitch.profileName}」`
                : " · 已写入客户端配置"}
        </StatusField>
      )}
      {(status.route?.scopeWarnings.length ?? 0) > 0 && (
        <StatusField label="范围警告">
          {status.route?.scopeWarnings.map((warning) => <span key={warning} className="asb-warn-text asb-status-warn">{warning}</span>)}
        </StatusField>
      )}
      <StatusField label="写入锁">{lockLabel(lock)}</StatusField>
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
  const pill = statusPill(status);
  return (
    <article className="asb-client-status" aria-label={`${clientName(status.app)} 配置状态`}>
      <header className="asb-client-status-head">
        <span className="asb-client-status-name"><ClientLogo app={status.app} className="asb-status-logo" />{clientName(status.app)}</span>
        <span className={`asb-status-pill${pill.ok ? " is-ok" : ""}`}>
          <span className="asb-status-pill-dot" aria-hidden="true" />{pill.text}
        </span>
      </header>
      <ConfigStatusDetails status={status} profiles={profiles} lock={lock} />
      {lock?.state === "stale" && (
        <div className="asb-kv-actions">
          <Button variant="secondary" disabled={busy} onClick={() => onRecoverLock(status.app)}>清理遗留锁</Button>
        </div>
      )}
    </article>
  );
}

export function ConfigStatusPanel({ statuses, profiles, locks, busy, onRecoverLock }: ConfigStatusPanelProps) {
  return (
    <section className="asb-panel" aria-labelledby="configuration-status-heading">
      <ModuleHeader id="configuration-status-heading" title="配置状态" />
      {statuses === null ? (
        /* Two client sections at the real anatomy's footprint: a head line and
           five fact lines each, so the panel does not reflow on arrival. */
        <div role="status" aria-label="正在读取配置状态">
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
