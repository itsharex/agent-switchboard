import { useMessageState } from "../i18n/use-message-state";
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
import { useI18n, type MessageKey } from "../i18n";
import { NATIVE_PROTOCOL, PROTOCOL_LABELS, isProtocolTranslation } from "../lib/protocol";
import {
  discardGatewayPortChange,
  getGatewayStatus,
  retryGatewayBind,
  type AppKind,
  type GatewayRouteStatus,
  type GatewaySample,
  type GatewayStatus,
  type GatewayStatusKind,
  type ProviderProfile,
} from "../api/client";

const POLL_INTERVAL_MS = 5000;
const TRAFFIC_WINDOW_MS = 3_600_000;
const APP_ORDER: AppKind[] = ["codex", "claude"];
const APP_LABELS: Record<AppKind, string> = { codex: "Codex", claude: "Claude Code" };

const STATUS_KEYS: Record<GatewayStatusKind, MessageKey> = {
  standby: "gateway.status.standby",
  running: "gateway.status.running",
  portConflict: "gateway.status.portConflict",
  bindRejected: "gateway.status.bindRejected",
  needsRepair: "gateway.status.needsRepair",
  recoveryBlocked: "gateway.status.recoveryBlocked",
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

/** 本机协议网关的运行面：一整张拓扑主图同时回答「活着没、谁在经过」——
 * 客户端、网关与供应商画在同一张图上，网关节点是唯一视觉锚点，左右母线
 * 组织接线；图下方的图表带回答「跑得好不好」。全部事实来自
 * `gateway_status` 快照，连线上的流量计数只来自内存遥测样本；端口修改
 * 始终走准备、预览和确认提交事务，本页不直接写入客户端配置。 */
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
      <GatewayMap
        status={status}
        failed={failed}
        refreshedAt={refreshedAt}
        retrying={retrying}
        onRetry={retryBind}
        onChangePort={() => setPortSheet({ stage: "input" })}
        onDiscardRecovery={discardRecovery}
        profileNames={profileNames}
      />
      <GatewayTelemetry metrics={status.metrics} profileNames={profileNames} />
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
  const [error, setError] = useMessageState();
  const [retrying, setRetrying] = useState(false);
  const refresh = useCallback(async () => {
    try {
      setStatus(await getGatewayStatus());
      setRefreshedAt(Date.now());
      setError(null);
    } catch (cause) {
      setError(cause);
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
      setError(cause);
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
      setError(cause);
    }
  }, []);

  return { status, error, retrying, refreshedAt, refresh, retryBind, discardRecovery };
}

function routeKey(route: Pick<GatewayRouteStatus, "app" | "profileId">): string {
  return `${route.app}:${route.profileId}`;
}

/** 连线流量计数：只统计近 60 分钟内匹配到活动路由的内存遥测样本。 */
function useTrafficCounts(samples: GatewaySample[]) {
  return useMemo(() => {
    const sinceMs = Date.now() - TRAFFIC_WINDOW_MS;
    const byRoute = new Map<string, number>();
    const byApp = new Map<AppKind, number>();
    for (const sample of samples) {
      if (sample.atMs < sinceMs || sample.profileId === null) continue;
      // 属性窄化不随整个对象引用传递，这里显式传入已收窄的字段对。
      const key = routeKey({ app: sample.app, profileId: sample.profileId });
      byRoute.set(key, (byRoute.get(key) ?? 0) + 1);
      byApp.set(sample.app, (byApp.get(sample.app) ?? 0) + 1);
    }
    return { byRoute, byApp };
  }, [samples]);
}

/** 拓扑主图：客户端列与供应商列分列左右，网关节点居中跨全部行，两条
 * 母线是接线的背板。每个客户端固定占据首末行，供应商按序占行；行数由
 * 路由数量决定，无路由时供应商列以虚线空位说明直连状态。 */
function GatewayMap({ status, failed, refreshedAt, retrying, onRetry, onChangePort, onDiscardRecovery, profileNames }: {
  status: GatewayStatus;
  failed: boolean;
  refreshedAt: number | null;
  retrying: boolean;
  onRetry: () => void;
  onChangePort: () => void;
  onDiscardRecovery: () => void;
  profileNames: Map<string, string>;
}) {
  const { t } = useI18n();
  const routes = status.routes;
  const rows = Math.max(APP_ORDER.length, routes.length);
  const clientRows = [1, rows];
  const traffic = useTrafficCounts(status.metrics.samples);
  const translated = routes.filter((route) =>
    isProtocolTranslation(NATIVE_PROTOCOL[route.app], route.upstreamProtocol)
  ).length;
  return (
    <section className={`asb-gateway-map${failed ? " is-failure" : ""}`} aria-label={t("gateway.mapAriaLabel")}>
      <header className="asb-gateway-map-heading">
        <h3 className="asb-section-title">{t("gateway.topologyTitle")}</h3>
        <span className="asb-gateway-section-count">
          {t("gateway.topologySummary", { translated, direct: routes.length - translated })}
        </span>
      </header>
      <div
        className="asb-gateway-map-canvas"
        style={{ gridTemplateRows: `repeat(${rows}, auto)` }}
      >
        <GatewayClientNodes rows={clientRows} counts={traffic.byApp} />
        <div className="asb-gateway-bus is-left" aria-hidden="true" />
        <GatewayHub
          status={status}
          failed={failed}
          refreshedAt={refreshedAt}
          retrying={retrying}
          onRetry={onRetry}
          onChangePort={onChangePort}
          rows={rows}
        />
        <div className="asb-gateway-bus is-right" aria-hidden="true" />
        <GatewayProviderNodes routes={routes} profileNames={profileNames} counts={traffic.byRoute} />
        {routes.length === 0 && (
          <div className="asb-gateway-node is-empty" style={{ gridRow: `1 / span ${rows}` }} role="status">
            <strong>{t("gateway.emptyRoutesTitle")}</strong>
            <span>{t("gateway.emptyRoutesCopy")}</span>
          </div>
        )}
      </div>
      {(status.status === "needsRepair" || status.blockedRecovery) && (
        <div className="asb-gateway-guidance">
          {status.status === "needsRepair" && <GatewayRepairGuidance status={status} />}
          {status.blockedRecovery && (
            <div className="asb-gateway-guidance-item" role="alert">
              <p className="asb-gateway-guidance-title">
                {t("gateway.recoveryBlockedTitle", {
                  from: status.blockedRecovery.fromPort,
                  to: status.blockedRecovery.toPort,
                  reason: status.blockedRecovery.reason,
                })}
              </p>
              <p className="asb-gateway-guidance-copy">
                {t("gateway.recoveryBlockedCopy")}
              </p>
              <ConfirmGatewayRecoveryDiscard onConfirm={() => void onDiscardRecovery()} />
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function GatewayClientNodes({ rows, counts }: { rows: number[]; counts: Map<AppKind, number> }) {
  const { t } = useI18n();
  return (
    <>
      {APP_ORDER.map((app, index) => (
        <div className="asb-gateway-connection is-client" key={app} style={{ gridRow: rows[index] }}>
          <div className="asb-gateway-node is-client" style={{ gridRow: rows[index] }}>
            <ClientLogo app={app} className="asb-gateway-node-logo" />
            <div>
              <span className="asb-gateway-node-caption">{t("gateway.client")}</span>
              <strong>{APP_LABELS[app]}</strong>
            </div>
          </div>
          <div className="asb-gateway-run is-client" style={{ gridRow: rows[index] }}>
            <span className="asb-gateway-chip">{PROTOCOL_LABELS[NATIVE_PROTOCOL[app]]}</span>
            <TrafficBadge count={counts.get(app) ?? 0} />
          </div>
        </div>
      ))}
    </>
  );
}

function GatewayProviderNodes({ routes, profileNames, counts }: {
  routes: GatewayRouteStatus[];
  profileNames: Map<string, string>;
  counts: Map<string, number>;
}) {
  const { t } = useI18n();
  return (
    <>
      {routes.map((route, index) => {
        const translated = isProtocolTranslation(NATIVE_PROTOCOL[route.app], route.upstreamProtocol);
        return (
          <div className="asb-gateway-connection is-provider" key={routeKey(route)} style={{ gridRow: index + 1 }}>
            <div
              className={`asb-gateway-run is-provider${translated ? " is-translation" : ""}`}
              style={{ gridRow: index + 1 }}
            >
              <span className="asb-gateway-chip">{PROTOCOL_LABELS[route.upstreamProtocol]}</span>
              <TrafficBadge count={counts.get(routeKey(route)) ?? 0} />
            </div>
            <div className="asb-gateway-node is-provider" style={{ gridRow: index + 1 }}>
              <span className="asb-gateway-node-caption">{t("gateway.providerCaption", { app: APP_LABELS[route.app] })}</span>
              <strong>{profileNames.get(route.profileId) ?? t("gateway.deletedProvider")}</strong>
              <span className="asb-gateway-route-protocol">
                {PROTOCOL_LABELS[NATIVE_PROTOCOL[route.app]]} → {PROTOCOL_LABELS[route.upstreamProtocol]}
              </span>
              <span className={`asb-gateway-mode${translated ? " is-translation" : ""}`}>
                {translated ? t("gateway.modeTranslation") : t("gateway.modeRelay")}
              </span>
            </div>
          </div>
        );
      })}
    </>
  );
}

/** 网关节点：全图唯一视觉锚点，状态灯、回环地址、端口事实与常驻动作都
 * 挂在节点上；失败时整体转为警示材质并内嵌失败信息。 */
function GatewayHub({ status, failed, refreshedAt, retrying, onRetry, onChangePort, rows }: {
  status: GatewayStatus;
  failed: boolean;
  refreshedAt: number | null;
  retrying: boolean;
  onRetry: () => void;
  onChangePort: () => void;
  rows: number;
}) {
  const { t } = useI18n();
  return (
    <section
      className={`asb-gateway-hub${failed ? " is-failure" : ""}`}
      style={{ gridRow: `1 / span ${rows}` }}
    >
      <div className="asb-gateway-hub-identity">
        <span className="asb-gateway-hub-symbol" aria-hidden="true"><GatewayIcon size={24} /></span>
        <div className="asb-gateway-hub-copy">
          <p className="asb-gateway-kicker">{t("gateway.title")}</p>
          <p className="asb-gateway-hub-state">
            <span className={`asb-gateway-lamp${failed ? " is-failure" : status.status === "running" ? " is-running" : ""}`} aria-hidden="true" />
            {t(STATUS_KEYS[status.status])}
          </p>
          <GatewayUpdatedAgo at={refreshedAt} />
        </div>
      </div>
      <div className="asb-gateway-hub-address">
        <p className="asb-gateway-kicker">{t("gateway.loopbackKicker")}</p>
        <div className={`asb-gateway-plate${status.baseUrl ? "" : " is-unavailable"}`}>
          <span className="asb-gateway-plate-address asb-code">{status.baseUrl ?? t("gateway.addressNotListening")}</span>
          {status.baseUrl && <CopyGatewayAddressButton value={status.baseUrl} />}
        </div>
      </div>
      {status.failure && <p className="asb-gateway-hub-message" role="alert">{status.failure.message}</p>}
      <dl className="asb-fact-row">
        <div><dt>{t("gateway.configuredPort")}</dt><dd className="asb-num">{status.configuredPort}</dd></div>
        <div><dt>{t("gateway.actualPort")}</dt><dd className="asb-num">{status.listeningPort ?? t("gateway.portNotListening")}</dd></div>
      </dl>
      <div className="asb-gateway-hub-actions">
        {status.failure && (
          <Button variant="secondary" disabled={retrying} onClick={() => void onRetry()}>{t("gateway.retryListen")}</Button>
        )}
        <Button variant="secondary" disabled={status.status === "recoveryBlocked"} onClick={onChangePort}>{t("gateway.changePort")}</Button>
      </div>
    </section>
  );
}

function TrafficBadge({ count }: { count: number }) {
  const { t } = useI18n();
  if (count === 0) return null;
  return (
    <span className="asb-gateway-run-count" aria-label={t("gateway.trafficBadge", { count })}>
      {count}
    </span>
  );
}

/** 「N 秒前更新」的独立计时器：每秒重渲染只发生在这个组件内。 */
function GatewayUpdatedAgo({ at }: { at: number | null }) {
  const { t } = useI18n();
  const [, setTick] = useState(0);
  useEffect(() => {
    if (at === null) return;
    const timer = window.setInterval(() => setTick((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [at]);
  if (at === null) return null;
  const seconds = Math.max(0, Math.round((Date.now() - at) / 1000));
  const text = seconds < 5
    ? t("gateway.updatedJustNow")
    : seconds < 60
      ? t("gateway.updatedSecondsAgo", { seconds })
      : t("gateway.updatedMinutesAgo", { minutes: Math.floor(seconds / 60) });
  return <p className="asb-gateway-hub-updated">{text}</p>;
}

/** 修复指引是主图的内嵌子条：附着在图下方，不另起平级告警卡。 */
function GatewayRepairGuidance({ status }: { status: GatewayStatus }) {
  const { t } = useI18n();
  return (
    <div className="asb-gateway-guidance-item" role="alert">
      <p className="asb-gateway-guidance-title">
        {status.repairReason ?? t("gateway.repairFallbackTitle")}
      </p>
      <p className="asb-gateway-guidance-copy">
        {status.listeningPort === null && t("gateway.repairRetryCopy")}
        {t("gateway.repairReapplyCopy")}
      </p>
    </div>
  );
}

function GatewayUnavailable({ error, onRetry }: { error: string; onRetry: () => void }) {
  const { t } = useI18n();
  return (
    <GatewayPanel>
      <div className="asb-gateway-unavailable" role="alert">
        <GatewayIcon />
        <div>
          <p className="asb-gateway-unavailable-title">{t("gateway.unavailableTitle")}</p>
          <p className="asb-gateway-error">{error}</p>
        </div>
        <Button variant="secondary" onClick={() => void onRetry()}>{t("gateway.retry")}</Button>
      </div>
    </GatewayPanel>
  );
}

/* Loading reserves the new page's footprint (map, strip, recent requests). */
function GatewayLoading() {
  const { t } = useI18n();
  return (
    <GatewayPanel>
      <div className="asb-gateway-loading" role="status" aria-label={t("gateway.loadingAriaLabel")}>
        <span className="asb-skeleton" aria-hidden="true" />
        <span className="asb-skeleton" aria-hidden="true" />
        <span className="asb-skeleton" aria-hidden="true" />
      </div>
    </GatewayPanel>
  );
}
