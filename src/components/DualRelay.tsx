import { useLayoutEffect, useRef } from "react";
import type { AppKind, ConfigFileStatus, LockStatus, ProviderProfile, RouteState } from "../api/client";
import { clientName } from "../lib/client-name";
import { currentProviderName } from "../lib/current-provider-name";
import { ClientLogo } from "./ClientLogo";
import "../styles/base/route-cards.css";

interface RouteCardProps {
  app: AppKind;
  status: ConfigFileStatus | undefined;
  profiles: ProviderProfile[];
  lock: LockStatus | undefined;
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
  const cardRef = useContinuousFlowPhase();
  const readable = status && !status.readError && status.exists && status.syntaxOk;
  const route = readable ? status.route : null;
  const notes = configurationNotes(status);
  const lockWarning = lockNote(lock);
  if (lockWarning) notes.push(lockWarning);
  return (
    <section ref={cardRef} className={`asb-route-card${route ? " is-on" : ""}`} data-app={app} aria-label={`${clientName(app)} 当前连接`}>
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
