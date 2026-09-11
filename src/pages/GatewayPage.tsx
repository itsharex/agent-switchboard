import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { GatewayPortChangeSheet, type PortSheetState } from "../components/GatewayPortChangeSheet";
import { GatewayTelemetry } from "../components/GatewayTelemetry";
import {
  ConfirmGatewayRecoveryDiscard,
  CopyGatewayAddressButton,
  GatewayPanel,
  GatewayStatTile,
} from "../components/GatewayPagePrimitives";
import { PROTOCOL_LABELS } from "../lib/protocol";
import {
  discardGatewayPortChange,
  getGatewayStatus,
  retryGatewayBind,
  type AppKind,
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

/** 网关页：本机协议网关的监听状态、端口管理、活动路由与请求遥测。全部事实
 * 来自后端 `gateway_status` 快照；端口修改走「准备 → 预览 → 确认提交」
 * 事务，本页自身不写任何文件。 */
export function GatewayPage({ active, profiles }: Props) {
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [portSheet, setPortSheet] = useState<PortSheetState | null>(null);

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

  const profileNames = useMemo(() => {
    const names = new Map<string, string>();
    for (const profile of profiles) names.set(profile.id, profile.name);
    return names;
  }, [profiles]);

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

  if (error) {
    return (
      <GatewayPanel>
        <p className="asb-gateway-error" role="alert">
          无法读取网关状态：{error}
        </p>
        <Button variant="secondary" onClick={() => void refresh()}>
          重试
        </Button>
      </GatewayPanel>
    );
  }
  if (!status) {
    return (
      <GatewayPanel>
        <p className="asb-gateway-empty" role="status">
          正在读取网关状态…
        </p>
      </GatewayPanel>
    );
  }

  const failed = FAILURE_STATES.has(status.status);

  return (
    <GatewayPanel
      aside={
        <span
          className={`asb-gateway-status${failed ? " is-failure" : ""}`}
          role="status"
          aria-label="网关运行状态"
        >
          {STATUS_LABELS[status.status]}
        </span>
      }
    >
      <div className="asb-gateway-info" aria-label="监听信息">
        <div className="asb-gateway-row">
          <span className="asb-gateway-row-label">监听地址</span>
          {status.baseUrl ? (
            <>
              <span className="asb-gateway-row-value asb-code">{status.baseUrl}</span>
              <CopyGatewayAddressButton value={status.baseUrl} />
            </>
          ) : (
            <span className="asb-gateway-empty">未在监听</span>
          )}
        </div>
        <div className="asb-gateway-row">
          <span className="asb-gateway-row-label">监听端口</span>
          <span className="asb-gateway-row-value">{status.configuredPort}</span>
          <Button
            variant="secondary"
            disabled={status.status === "recoveryBlocked"}
            onClick={() => setPortSheet({ stage: "input" })}
          >
            修改
          </Button>
        </div>
        <div className="asb-gateway-row">
          <span className="asb-gateway-row-label">使用客户端</span>
          <span>Codex、Claude Code</span>
        </div>
        <p className="asb-scope-note">
          修改监听端口会同步更新正在使用本网关的客户端配置；修改完成后，请重新启动相关客户端或会话。
        </p>
      </div>

      {status.failure && (
        <div role="alert" className="asb-gateway-alert">
          <p className="asb-gateway-alert-title">{status.failure.message}</p>
          <p className="asb-gateway-alert-copy">
            使用本网关的客户端当前无法连接；本应用仍可管理供应商、查看日志或切换到直连与官方登录。
          </p>
          <div className="asb-panel-actions">
            <Button variant="secondary" disabled={retrying} onClick={() => void retryBind()}>
              重试
            </Button>
            <Button
              variant="secondary"
              disabled={status.status === "recoveryBlocked"}
              onClick={() => setPortSheet({ stage: "input" })}
            >
              修改端口
            </Button>
          </div>
        </div>
      )}

      {status.status === "needsRepair" && (
        <div role="alert" className="asb-gateway-alert">
          <p className="asb-gateway-alert-title">
            {status.repairReason ?? "本机协议网关需要修复。客户端仍指向本网关时，状态损坏会保留诊断副本，身份不匹配会拒绝恢复旧路由。"}
          </p>
          <p className="asb-gateway-alert-copy">
            请在供应商页重新应用指向本网关的供应商：重新应用会写入新的服务地址与本机能力令牌，
            预览会先展示这些变化；不再使用网关的客户端可切换到直连或官方登录。
          </p>
        </div>
      )}

      {status.blockedRecovery && (
        <div role="alert" className="asb-gateway-alert">
          <p className="asb-gateway-alert-title">
            上次端口修改（{status.blockedRecovery.fromPort} →{" "}
            {status.blockedRecovery.toPort}）需要处理：{status.blockedRecovery.reason}
          </p>
          <p className="asb-gateway-alert-copy">
            已保留恢复记录与备份，不会覆盖当前配置；经网关的切换与新的端口修改已暂停。
          </p>
          <ConfirmGatewayRecoveryDiscard onConfirm={() => void discardRecovery()} />
        </div>
      )}

      <div className="asb-gateway-tiles">
        <GatewayStatTile label="累计请求" value={status.metrics.totalRequests} />
        <GatewayStatTile label="失败请求" value={status.metrics.failedRequests} />
        <GatewayStatTile label="活动路由" value={status.routes.length} />
        <GatewayStatTile
          label="实际监听"
          value={status.listeningPort ?? "未监听"}
        />
      </div>

      <section aria-label="活动路由" className="asb-gateway-section">
        <h3 className="asb-section-title">活动路由</h3>
        {status.routes.length === 0 ? (
          <p className="asb-gateway-empty" role="status">
            当前没有经本机协议网关转换的供应商；客户端均在直连或官方登录。
          </p>
        ) : (
          <ul className="asb-gateway-route-list">
            {status.routes.map((route) => (
              <li key={`${route.app}:${route.profileId}`}>
                <span className="asb-gateway-route-client">{APP_LABELS[route.app]}</span>
                <span>{profileNames.get(route.profileId) ?? "已删除的供应商"}</span>
                <span>上游 {PROTOCOL_LABELS[route.upstreamProtocol]}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

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
