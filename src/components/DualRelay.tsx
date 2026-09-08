import type { AppKind, ConfigFileStatus, LockStatus, ProviderProfile, RouteState } from "../api/client";
import type { DiagnosticSection } from "../app/navigation";
import { clientName } from "../lib/client-name";
import { currentProviderName } from "../lib/current-provider-name";
import { requiresGateway } from "../lib/protocol";
import { Button } from "./Button";
import { ClientLogo } from "./ClientLogo";
import { StarlightLayer } from "./experience/StarlightLayer";
import "../styles/base/route-cards.css";

interface RouteCardProps {
  app: AppKind;
  status: ConfigFileStatus | undefined;
  profiles: ProviderProfile[];
  lock: LockStatus | undefined;
  onOpenDiagnostics: (section: DiagnosticSection) => void;
  onOpenQuota: () => void;
}

function configurationNotes(status: ConfigFileStatus | undefined): string[] {
  if (!status) return ["配置状态尚未读取"];
  if (status.readError) return [`读取失败：${status.readError}`];
  if (!status.exists) return ["未找到配置文件"];
  if (!status.syntaxOk) return ["配置语法错误"];
  const notes: string[] = [];
  switch (status.matchStatus.kind) {
    case "externallyModified": notes.push("配置已被外部修改，与上次切换不一致"); break;
    case "profileChanged": notes.push("档案或客户端偏好已更改，尚未应用"); break;
    case "unmanaged": notes.push("当前配置未匹配已存档案"); break;
    case "restoredBackup": notes.push("当前配置来自已恢复的备份"); break;
    case "unknown": notes.push("配置匹配状态无法确定"); break;
  }
  return [...notes, ...(status.route?.scopeWarnings ?? [])];
}

function lockNote(lock: LockStatus | undefined): string | null {
  if (!lock) return "写入锁状态尚未读取";
  switch (lock.state) {
    case "free": return null;
    case "stale": return "发现遗留写入锁，可在诊断中清理";
    case "held": return `写入锁由${lock.processName ?? (lock.pid ? `进程 ${lock.pid}` : "其他进程")}持有`;
    case "indeterminate": return `写入锁状态无法确定：${lock.reason}`;
  }
}

function accessLabel(route: RouteState | null): string {
  if (!route) return "未读取";
  return route.routeMode === "official" ? "官方登录" : "自定义服务";
}

/** Summarizes observed configuration facts without treating valid syntax as health. */
function RouteCard({
  app, status, profiles, lock, onOpenDiagnostics, onOpenQuota,
}: RouteCardProps) {
  const readable = status && !status.readError && status.exists && status.syntaxOk;
  const route = readable ? status.route : null;
  const activeProfile = profiles.find((profile) => profile.app === app && profile.id === status?.activeProfileId);
  const notes = configurationNotes(status);
  const lockWarning = lockNote(lock);
  if (lockWarning) notes.push(lockWarning);
  return (
    <section className={`asb-route-card${route ? " is-on" : ""}`} data-app={app} aria-label={`${clientName(app)} 当前连接`}>
      {route && <StarlightLayer variant={app === "codex" ? "cool" : "violet"} />}
      <div className="asb-route-card-body">
        <div>
          <div className="asb-route-ident">
            <ClientLogo app={app} className="asb-route-logo" />
            <span className="asb-route-client">{clientName(app)}</span>
          </div>
          <h3 className="asb-route-provider">{route ? currentProviderName(status, profiles) : "未读取"}</h3>
        </div>
        <dl className="asb-route-values">
          <div><dt className="asb-route-key">模型</dt><dd className="asb-route-value">{route ? route.model ?? "客户端默认" : "未读取"}</dd></div>
          <div><dt className="asb-route-key">服务地址</dt><dd className="asb-route-value">{serviceAddress(route)}</dd></div>
          <div><dt className="asb-route-key">接入方式</dt><dd className="asb-route-value">{accessLabel(route)}</dd></div>
        </dl>
        {notes.length > 0 && (
          <ul className="asb-route-notes">
            {notes.map((note, index) => <li key={`${index}-${note}`}>{note}</li>)}
          </ul>
        )}
        <div className="asb-route-actions">
          <Button variant="secondary" onClick={() => onOpenDiagnostics("configuration")}>
            {notes.length > 0 ? "查看诊断" : "配置与环境"}
          </Button>
          {route && activeProfile && requiresGateway(activeProfile) && (
            <Button variant="secondary" onClick={() => onOpenDiagnostics("gateway")}>网关诊断</Button>
          )}
          {app === "codex" && route?.routeMode === "official" && (
            <Button variant="secondary" onClick={onOpenQuota}>官方额度详情</Button>
          )}
        </div>
      </div>
    </section>
  );
}

function serviceAddress(route: RouteState | null): string {
  if (!route) return "未读取";
  if (!route.baseUrl) return route.routeMode === "official" ? "官方服务" : "未设置";
  try { return new URL(route.baseUrl).host; } catch { return "地址无法解析"; }
}

interface DualRelayProps {
  statuses: ConfigFileStatus[] | null;
  profiles: ProviderProfile[];
  locks: Partial<Record<AppKind, LockStatus>>;
  onOpenDiagnostics: RouteCardProps["onOpenDiagnostics"];
  onOpenQuota: () => void;
}

/** Both cards describe observed client files, independently of list selection. */
export function DualRelay({ statuses, profiles, locks, onOpenDiagnostics, onOpenQuota }: DualRelayProps) {
  return (
    <div className="asb-route-cards" role="group" aria-label="当前启用配置">
      {(["codex", "claude"] as const).map((app) => (
        <RouteCard key={app} app={app} status={statuses?.find((status) => status.app === app)}
          profiles={profiles} lock={locks[app]} onOpenDiagnostics={onOpenDiagnostics} onOpenQuota={onOpenQuota} />
      ))}
    </div>
  );
}
