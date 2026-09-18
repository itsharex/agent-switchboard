import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { ClientLogo } from "../components/ClientLogo";
import { GatewayPortChangeSheet, type PortSheetState } from "../components/GatewayPortChangeSheet";
import { GatewayTelemetry } from "../components/GatewayTelemetry";
import {
  ConfirmGatewayRecoveryDiscard,
  CopyGatewayAddressButton,
  GatewayPanel,
} from "../components/GatewayPagePrimitives";
import { GatewayIcon } from "../components/icons";
import { NATIVE_PROTOCOL, PROTOCOL_LABELS, isProtocolTranslation } from "../lib/protocol";
import {
  discardGatewayPortChange,
  getGatewayStatus,
  retryGatewayBind,
  type AppKind,
  type GatewayRouteStatus,
  type GatewayStatus,
  type GatewayStatusKind,
  type ProviderProfile,
} from "../api/client";

const POLL_INTERVAL_MS = 5000;
const APP_LABELS: Record<AppKind, string> = { codex: "Codex", claude: "Claude Code" };

const STATUS_LABELS: Record<GatewayStatusKind, string> = {
  standby: "待命",
  running: "运行中",
  portConflict: "端口冲突",
  bindRejected: "绑定被拒绝",
  needsRepair: "需要修复",
  recoveryBlocked: "需要恢复",
};

const FAILURE_STATES: ReadonlySet<GatewayStatusKind> = new Set([
  "portConflict",
  "bindRejected",
  "needsRepair",
  "recoveryBlocked",
]);

interface Props {
  /** False while another page is shown; polling runs only when visible. */
  active: boolean;
  profiles: ProviderProfile[];
}

/** 本机协议网关的运行面，按「主机面—接线排—仪表带」组织：主机面回答
 * 「活着没」，接线排回答「谁在经过」，仪表带回答「跑得好不好」。全部
 * 事实来自 `gateway_status` 快照；端口修改始终走准备、预览和确认提交
 * 事务，本页不直接写入客户端配置。 */
export function GatewayPage({ active, profiles }: Props) {
  const [portSheet, setPortSheet] = useState<PortSheetState | null>(null);
  const { status, error, retrying, refreshedAt, refresh, retryBind, discardRecovery } = useGatewayRuntime(active);
  const profileNames = useMemo(() => {
    const names = new Map<string, string>();
    for (const profile of profiles) names.set(profile.id, profile.name);
    return names;
  }, [profiles]);

  if (error) return <GatewayUnavailable error={error} onRetry={refresh} />;
  if (!status) return <GatewayLoading />;

  const failed = FAILURE_STATES.has(status.status);
  return (
    <GatewayPanel>
      <GatewayFace
        status={status}
        failed={failed}
        refreshedAt={refreshedAt}
        retrying={retrying}
        onRetry={retryBind}
        onChangePort={() => setPortSheet({ stage: "input" })}
      />
      <GatewayAlerts status={status} onDiscardRecovery={discardRecovery} />
      <GatewayRouteBoard routes={status.routes} profileNames={profileNames} />
      <GatewayTelemetry
        metrics={status.metrics}
        routeCount={status.routes.length}
        profileNames={profileNames}
      />
      {portSheet && (
        <GatewayPortChangeSheet
          state={portSheet}
          configuredPort={status.configuredPort}
          onState={setPortSheet}
          onRefreshed={() => void refresh()}
        />
      )}
    </GatewayPanel>
  );
}

function useGatewayRuntime(active: boolean) {
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [refreshedAt, setRefreshedAt] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const refresh = useCallback(async () => {
    try {
      setStatus(await getGatewayStatus());
      setRefreshedAt(Date.now());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [active, refresh]);

  const retryBind = useCallback(async () => {
    setRetrying(true);
    try {
      setStatus(await retryGatewayBind());
      setRefreshedAt(Date.now());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRetrying(false);
    }
  }, []);

  const discardRecovery = useCallback(async () => {
    try {
      setStatus(await discardGatewayPortChange(true));
      setRefreshedAt(Date.now());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  return { status, error, retrying, refreshedAt, refresh, retryBind, discardRecovery };
}

function GatewayAlerts({ status, onDiscardRecovery }: {
  status: GatewayStatus;
  onDiscardRecovery: () => void;
}) {
  return (
    <>
      {status.status === "needsRepair" && <GatewayRepairAlert status={status} />}
      {status.blockedRecovery && (
        <div role="alert" className="asb-gateway-alert">
          <p className="asb-gateway-alert-title">上次端口修改（{status.blockedRecovery.fromPort} → {status.blockedRecovery.toPort}）需要处理：{status.blockedRecovery.reason}</p>
          <p className="asb-gateway-alert-copy">已保留恢复记录与备份，不会覆盖当前配置；经网关的切换与新的端口修改已暂停。</p>
          <ConfirmGatewayRecoveryDiscard onConfirm={() => void onDiscardRecovery()} />
        </div>
      )}
    </>
  );
}

function GatewayRepairAlert({ status }: { status: GatewayStatus }) {
  return (
    <div role="alert" className="asb-gateway-alert">
      <p className="asb-gateway-alert-title">{status.repairReason ?? "本机协议网关需要修复。客户端仍指向本网关时，状态损坏会保留诊断副本，身份不匹配会拒绝恢复旧路由。"}</p>
      <p className="asb-gateway-alert-copy">
        {status.listeningPort === null && "先点击重试恢复监听；状态无法读取时会先保存诊断副本，再重建网关状态。"}
        请在供应商页重新应用指向本网关的供应商：预览会先展示新的服务地址与本机能力令牌；不再使用网关的客户端可切换到直连或官方登录。
      </p>
    </div>
  );
}

function GatewayUnavailable({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <GatewayPanel>
      <div className="asb-gateway-unavailable" role="alert">
        <GatewayIcon />
        <div>
          <p className="asb-gateway-unavailable-title">无法读取网关状态</p>
          <p className="asb-gateway-error">{error}</p>
        </div>
        <Button variant="secondary" onClick={() => void onRetry()}>重试</Button>
      </div>
    </GatewayPanel>
  );
}

function GatewayLoading() {
  return (
    <GatewayPanel>
      <div className="asb-gateway-loading" role="status" aria-label="正在读取">
        {Array.from({ length: 4 }, (_, index) => (
          <span key={index} className="asb-skeleton" aria-hidden="true" />
        ))}
      </div>
    </GatewayPanel>
  );
}

/** 主机面：网关页唯一的视觉锚点。状态灯与状态词回答「活着没」，等宽
 * 地址牌承载回环访问地址，失败时整个面转为警示材质并内嵌恢复动作。 */
function GatewayFace({ status, failed, refreshedAt, retrying, onRetry, onChangePort }: {
  status: GatewayStatus;
  failed: boolean;
  refreshedAt: number | null;
  retrying: boolean;
  onRetry: () => void;
  onChangePort: () => void;
}) {
  return (
    <section className={`asb-gateway-face${failed ? " is-failure" : ""}`} aria-label="网关主机面">
      <div className="asb-gateway-face-primary">
        <div className="asb-gateway-face-identity">
          <span
            aria-hidden="true"
            className={[
              "asb-gateway-lamp",
              failed ? "is-failure" : "",
              status.status === "running" ? "is-running" : "",
            ].filter(Boolean).join(" ")}
          >
            <span className="asb-gateway-lamp-pulse" />
            <span className="asb-gateway-lamp-core" />
          </span>
          <div className="asb-gateway-face-copy">
            <p className="asb-gateway-face-label">本机协议网关</p>
            <p className="asb-gateway-face-state">{STATUS_LABELS[status.status]}</p>
            <GatewayUpdatedAgo at={refreshedAt} />
          </div>
        </div>
        <div className="asb-gateway-face-address">
          <p className="asb-gateway-face-label">回环访问地址</p>
          <div className={`asb-gateway-plate${status.baseUrl ? "" : " is-unavailable"}`}>
            <span className="asb-gateway-plate-address asb-code">{status.baseUrl ?? "未在监听"}</span>
            {status.baseUrl && <CopyGatewayAddressButton value={status.baseUrl} />}
          </div>
        </div>
      </div>
      {status.failure && <p className="asb-gateway-face-message" role="alert">{status.failure.message}</p>}
      <div className="asb-gateway-face-meta">
        <dl className="asb-fact-row">
          <div><dt>配置端口</dt><dd className="asb-num">{status.configuredPort}</dd></div>
          <div><dt>实际监听</dt><dd className="asb-num">{status.listeningPort ?? "未监听"}</dd></div>
        </dl>
        <div className="asb-gateway-face-actions">
          {status.failure && (
            <Button variant="secondary" disabled={retrying} onClick={() => void onRetry()}>重试监听</Button>
          )}
          <Button variant="secondary" disabled={status.status === "recoveryBlocked"} onClick={onChangePort}>修改端口</Button>
        </div>
      </div>
    </section>
  );
}

/** 「N 秒前更新」的独立计时器：每秒重渲染只发生在这个组件内。 */
function GatewayUpdatedAgo({ at }: { at: number | null }) {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (at === null) return;
    const timer = window.setInterval(() => setTick((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [at]);
  if (at === null) return null;
  const seconds = Math.max(0, Math.round((Date.now() - at) / 1000));
  const text = seconds < 5
    ? "刚刚更新"
    : seconds < 60
      ? `${seconds} 秒前更新`
      : `${Math.floor(seconds / 60)} 分钟前更新`;
  return <p className="asb-gateway-face-updated">{text}</p>;
}

function GatewayRouteBoard({ routes, profileNames }: {
  routes: GatewayRouteStatus[];
  profileNames: Map<string, string>;
}) {
  const translated = routes.filter((route) => isProtocolTranslation(NATIVE_PROTOCOL[route.app], route.upstreamProtocol)).length;
  const forwarded = routes.length - translated;
  return (
    <section aria-label="活动路由" className="asb-gateway-wiring">
      <div className="asb-gateway-wiring-heading">
        <div>
          <p className="asb-gateway-section-kicker">当前路径</p>
          <h3 className="asb-section-title">网关正在处理什么</h3>
        </div>
        <span className="asb-gateway-section-count">{translated} 条协议转换 · {forwarded} 条本机转发</span>
      </div>
      {routes.length === 0 ? (
        <p className="asb-gateway-empty" role="status">当前没有经本机网关处理的供应商；客户端均在直连或官方登录。</p>
      ) : (
        <ul className="asb-gateway-wire-list">
          {routes.map((route) => <GatewayRoute key={`${route.app}:${route.profileId}`} route={route} profileNames={profileNames} />)}
        </ul>
      )}
    </section>
  );
}

/** 接线排的一行：客户端 — 协议芯片挂在连线上 — 供应商节点。协议转换用
 * 蓝线表达，本机转发用中性线；两种模式始终有文字标签。 */
function GatewayRoute({ route, profileNames }: { route: GatewayRouteStatus; profileNames: Map<string, string> }) {
  const clientProtocol = NATIVE_PROTOCOL[route.app];
  const translated = isProtocolTranslation(clientProtocol, route.upstreamProtocol);
  const mode = translated ? "协议转换" : "本机转发";
  return (
    <li className={`asb-gateway-wire${translated ? " is-translation" : ""}`}>
      <div className="asb-gateway-wire-client">
        <ClientLogo app={route.app} className="asb-gateway-wire-logo" />
        <div>
          <span>客户端</span>
          <strong>{APP_LABELS[route.app]}</strong>
        </div>
      </div>
      <div className="asb-gateway-wire-line">
        <span className="asb-gateway-wire-protocol">{PROTOCOL_LABELS[clientProtocol]}</span>
        <span className="asb-gateway-wire-sep" aria-hidden="true">→</span>
        <span className="asb-gateway-wire-protocol">{PROTOCOL_LABELS[route.upstreamProtocol]}</span>
        <span className="asb-gateway-wire-mode">{mode}</span>
      </div>
      <div className="asb-gateway-wire-provider">
        <span>上游供应商</span>
        <strong>{profileNames.get(route.profileId) ?? "已删除的供应商"}</strong>
      </div>
    </li>
  );
}
