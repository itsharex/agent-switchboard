import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { ClientLogo } from "../components/ClientLogo";
import { GatewayPortChangeSheet, type PortSheetState } from "../components/GatewayPortChangeSheet";
import { GatewayTelemetry } from "../components/GatewayTelemetry";
import {
  ConfirmGatewayRecoveryDiscard,
  CopyGatewayAddressButton,
  GatewayPanel,
  GatewayStatTile,
} from "../components/GatewayPagePrimitives";
import { GatewayIcon, GatewaySignalIcon } from "../components/icons";
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

/** 本机协议网关的运行面。全部事实来自 `gateway_status` 快照；端口修改始终
 * 走准备、预览和确认提交事务，本页不直接写入客户端配置。 */
export function GatewayPage({ active, profiles }: Props) {
  const [portSheet, setPortSheet] = useState<PortSheetState | null>(null);
  const { status, error, retrying, refresh, retryBind, discardRecovery } = useGatewayRuntime(active);
  const profileNames = useMemo(() => {
    const names = new Map<string, string>();
    for (const profile of profiles) names.set(profile.id, profile.name);
    return names;
  }, [profiles]);

  if (error) return <GatewayUnavailable error={error} onRetry={refresh} />;
  if (!status) return <GatewayLoading />;

  const failed = FAILURE_STATES.has(status.status);
  return (
    <GatewayPanel aside={<GatewayStatus status={status.status} failed={failed} />}>
      <GatewayOverview status={status} failed={failed} routes={status.routes} onChangePort={() => setPortSheet({ stage: "input" })} />
      <GatewayAlerts
        status={status}
        retrying={retrying}
        onRetry={retryBind}
        onChangePort={() => setPortSheet({ stage: "input" })}
        onDiscardRecovery={discardRecovery}
      />
      <section className="asb-gateway-metrics" aria-label="网关计数">
        <GatewayStatTile label="累计请求" value={status.metrics.totalRequests} detail="本次启动" />
        <GatewayStatTile label="失败请求" value={status.metrics.failedRequests} detail="已完成请求" tone={status.metrics.failedRequests > 0 ? "warning" : "default"} />
        <GatewayStatTile label="活动路由" value={status.routes.length} detail="正在转发" />
        <GatewayStatTile label="实际监听" value={status.listeningPort ?? "未监听"} detail="回环端口" tone={failed ? "warning" : "default"} />
      </section>
      <GatewayRouteBoard routes={status.routes} profileNames={profileNames} />
      <GatewayTelemetry samples={status.metrics.samples} profileNames={profileNames} />
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
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const refresh = useCallback(async () => {
    try {
      setStatus(await getGatewayStatus());
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
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  return { status, error, retrying, refresh, retryBind, discardRecovery };
}

function GatewayAlerts({
  status,
  retrying,
  onRetry,
  onChangePort,
  onDiscardRecovery,
}: {
  status: GatewayStatus;
  retrying: boolean;
  onRetry: () => void;
  onChangePort: () => void;
  onDiscardRecovery: () => void;
}) {
  return (
    <>
      {status.failure && (
        <div role="alert" className="asb-gateway-alert">
          <p className="asb-gateway-alert-title">{status.failure.message}</p>
          <p className="asb-gateway-alert-copy">使用本网关的客户端当前无法连接；本应用仍可管理供应商、查看日志或切换到直连与官方登录。</p>
          <div className="asb-panel-actions">
            <Button variant="secondary" disabled={retrying} onClick={() => void onRetry()}>重试监听</Button>
            <Button variant="secondary" disabled={status.status === "recoveryBlocked"} onClick={onChangePort}>修改端口</Button>
          </div>
        </div>
      )}
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
      <div className="asb-gateway-loading" role="status">
        <GatewaySignalIcon />
        <span>正在读取本机网关状态…</span>
      </div>
    </GatewayPanel>
  );
}

function GatewayStatus({ status, failed }: { status: GatewayStatusKind; failed: boolean }) {
  return (
    <span className={`asb-gateway-status${failed ? " is-failure" : ""}`} role="status" aria-label="网关运行状态">
      <span aria-hidden="true" className="asb-gateway-status-dot" />
      {STATUS_LABELS[status]}
    </span>
  );
}

function GatewayOverview({ status, failed, routes, onChangePort }: {
  status: GatewayStatus;
  failed: boolean;
  routes: GatewayRouteStatus[];
  onChangePort: () => void;
}) {
  const endpoint = status.baseUrl ?? "未在监听";
  const routeSummary = routes.length === 0 ? "当前没有客户端经本网关处理" : `当前正在处理 ${routes.length} 条客户端路由`;
  return (
    <section className="asb-gateway-console" aria-label="网关概览">
      <div className="asb-gateway-console-identity">
        <span className={`asb-gateway-console-mark${failed ? " is-failure" : ""}`} aria-hidden="true"><GatewayIcon /></span>
        <div className="asb-gateway-console-copy">
          <p className="asb-gateway-console-label">本机协议网关</p>
          <h3 className="asb-gateway-console-title">{failed ? "监听需要处理" : "正在接收本机请求"}</h3>
          <p>{routeSummary}</p>
        </div>
      </div>
      <div className="asb-gateway-console-endpoint">
        <div className="asb-gateway-console-endpoint-heading">
          <p className="asb-gateway-console-label">回环访问地址</p>
          {status.baseUrl && <CopyGatewayAddressButton value={status.baseUrl} />}
        </div>
        <p className={`asb-gateway-console-address asb-code${status.baseUrl ? "" : " is-unavailable"}`}>{endpoint}</p>
        <dl className="asb-gateway-console-facts">
          <div><dt>配置端口</dt><dd>{status.configuredPort}</dd></div>
          <div><dt>实际监听</dt><dd>{status.listeningPort ?? "未监听"}</dd></div>
          <div><dt>路由状态</dt><dd>{routes.length === 0 ? "待命" : `${routes.length} 条活动路由`}</dd></div>
        </dl>
        <Button variant="secondary" disabled={status.status === "recoveryBlocked"} onClick={onChangePort}>修改端口</Button>
      </div>
    </section>
  );
}

function GatewayRouteBoard({ routes, profileNames }: {
  routes: GatewayRouteStatus[];
  profileNames: Map<string, string>;
}) {
  const translated = routes.filter((route) => isProtocolTranslation(NATIVE_PROTOCOL[route.app], route.upstreamProtocol)).length;
  const forwarded = routes.length - translated;
  return (
    <section aria-label="活动路由" className="asb-gateway-route-board">
      <div className="asb-gateway-route-board-heading">
        <div>
          <p className="asb-gateway-section-kicker">当前路径</p>
          <h3 className="asb-section-title">网关正在处理什么</h3>
        </div>
        <span className="asb-gateway-section-count">{translated} 条协议转换 · {forwarded} 条本机转发</span>
      </div>
      {routes.length === 0 ? (
        <p className="asb-gateway-empty" role="status">当前没有经本机网关处理的供应商；客户端均在直连或官方登录。</p>
      ) : (
        <ul className="asb-gateway-route-list">
          {routes.map((route) => <GatewayRoute key={`${route.app}:${route.profileId}`} route={route} profileNames={profileNames} />)}
        </ul>
      )}
    </section>
  );
}

function GatewayRoute({ route, profileNames }: { route: GatewayRouteStatus; profileNames: Map<string, string> }) {
  const clientProtocol = NATIVE_PROTOCOL[route.app];
  const translated = isProtocolTranslation(clientProtocol, route.upstreamProtocol);
  const mode = translated ? "协议转换" : "本机转发";
  return (
    <li className={`asb-gateway-route${translated ? " is-translation" : ""}`}>
      <div className="asb-gateway-route-client">
        <ClientLogo app={route.app} className="asb-gateway-route-logo" />
        <div>
          <span>客户端</span>
          <strong>{APP_LABELS[route.app]}</strong>
        </div>
      </div>
      <div className="asb-gateway-route-conversion" aria-label={`协议路径：${PROTOCOL_LABELS[clientProtocol]} 到 ${PROTOCOL_LABELS[route.upstreamProtocol]}，${mode}`}>
        <span>{PROTOCOL_LABELS[clientProtocol]}</span>
        <span className="asb-gateway-route-arrow" aria-hidden="true">→</span>
        <span>{PROTOCOL_LABELS[route.upstreamProtocol]}</span>
        <span className="asb-gateway-route-mode">{mode}</span>
      </div>
      <div className="asb-gateway-route-provider">
        <span>上游供应商</span>
        <strong>{profileNames.get(route.profileId) ?? "已删除的供应商"}</strong>
      </div>
    </li>
  );
}
