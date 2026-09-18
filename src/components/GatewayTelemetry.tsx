import { useState } from "react";
import type { GatewayMetricsStatus, GatewaySample, UpstreamProtocol } from "../api/client";
import { PROTOCOL_LABELS, isProtocolTranslation } from "../lib/protocol";
import { ChevronDownIcon } from "./icons";
import { Table, type TableColumn } from "./Table";

const RECENT_ROW_COUNT = 20;
const TREND_MINUTES = 60;
const MINUTE_MS = 60_000;
const SPARKLINE_WIDTH = 240;
const SPARKLINE_HEIGHT = 36;
const APP_LABELS = { codex: "Codex", claude: "Claude Code" } as const;
const PROTOCOL_TONES: Record<UpstreamProtocol, string> = {
  responses: "var(--asb-action)",
  chatCompletions: "var(--asb-claude)",
  anthropicMessages: "var(--asb-safe)",
  geminiGenerateContent: "var(--asb-warning)",
};

function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

interface TelemetryProps {
  metrics: GatewayMetricsStatus;
  routeCount: number;
  profileNames: Map<string, string>;
}

/** 仪表带是网关页的遥测主面：等宽大数字呈现累计请求（附近 60 分钟迷你
 * 趋势）、失败请求、p50/p95 耗时与上游协议分布；「最近请求」明细收进
 * 按需展开的披露区，展开时才挂载。全部读数只来自内存遥测样本。 */
export function GatewayTelemetry({ metrics, routeCount, profileNames }: TelemetryProps) {
  return (
    <>
      <GatewayInstruments metrics={metrics} routeCount={routeCount} />
      <GatewayRecentRequests samples={metrics.samples} profileNames={profileNames} />
    </>
  );
}

function GatewayInstruments({ metrics, routeCount }: {
  metrics: GatewayMetricsStatus;
  routeCount: number;
}) {
  const { totalRequests, failedRequests, samples } = metrics;
  const completed = samples
    .filter((sample) => sample.status !== null)
    .map((sample) => sample.durationMs)
    .sort((a, b) => a - b);
  const hasLatency = completed.length > 0;
  const p50 = hasLatency ? formatDuration(percentile(completed, 50)) : "—";
  const p95 = hasLatency ? formatDuration(percentile(completed, 95)) : "—";
  const usage = buildProtocolUsage(samples);
  return (
    <section className="asb-gateway-instruments" aria-label="网关仪表">
      <div className="asb-gateway-instrument is-trend">
        <p className="asb-gateway-instrument-label">累计请求</p>
        <p className="asb-gateway-instrument-value">{totalRequests}</p>
        <p className="asb-gateway-instrument-detail">本次启动 · 近 60 分钟</p>
        <GatewaySparkline samples={samples} />
      </div>
      <div className={`asb-gateway-instrument${failedRequests > 0 ? " is-warning" : ""}`}>
        <p className="asb-gateway-instrument-label">失败请求</p>
        <p className="asb-gateway-instrument-value">{failedRequests}</p>
        <p className="asb-gateway-instrument-detail">已完成请求</p>
      </div>
      <div className="asb-gateway-instrument">
        <p className="asb-gateway-instrument-label">请求耗时</p>
        <p className="asb-gateway-instrument-value">{p50}</p>
        <p className="asb-gateway-instrument-detail">{hasLatency ? `p95 ${p95} · 已完成请求` : "暂无已完成请求"}</p>
      </div>
      <div className="asb-gateway-instrument">
        <p className="asb-gateway-instrument-label">活动路由</p>
        <p className="asb-gateway-instrument-value">{routeCount}</p>
        <p className="asb-gateway-instrument-detail">正在转发</p>
      </div>
      <div className="asb-gateway-instrument is-protocol">
        <p className="asb-gateway-instrument-label">上游协议分布</p>
        {usage.length === 0 ? (
          <p className="asb-gateway-empty" role="status">暂无网关请求记录。</p>
        ) : (
          <>
            <div className="asb-gateway-protocol-bar" aria-hidden="true">
              {usage.map((entry) => (
                <span
                  key={entry.protocol}
                  style={{
                    flexGrow: entry.count,
                    backgroundColor: PROTOCOL_TONES[entry.protocol],
                  }}
                />
              ))}
            </div>
            <p className="asb-gateway-protocol-legend">
              {usage
                .map((entry) => `${PROTOCOL_LABELS[entry.protocol]} ${entry.count}`)
                .join(" · ")}
            </p>
          </>
        )}
      </div>
    </section>
  );
}

function GatewaySparkline({ samples }: { samples: GatewaySample[] }) {
  const buckets = buildMinuteBuckets(samples);
  const total = buckets.reduce((sum, count) => sum + count, 0);
  const max = Math.max(...buckets, 1);
  const step = SPARKLINE_WIDTH / (TREND_MINUTES - 1);
  const points = buckets
    .map((count, index) => {
      const x = index * step;
      const y = SPARKLINE_HEIGHT - 2 - (count / max) * (SPARKLINE_HEIGHT - 6);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg
      className="asb-gateway-sparkline"
      viewBox={`0 0 ${SPARKLINE_WIDTH} ${SPARKLINE_HEIGHT}`}
      preserveAspectRatio="none"
      aria-hidden="true"
      focusable="false"
    >
      {total === 0 ? (
        <line
          x1="0"
          y1={SPARKLINE_HEIGHT - 1}
          x2={SPARKLINE_WIDTH}
          y2={SPARKLINE_HEIGHT - 1}
          className="asb-gateway-sparkline-baseline"
        />
      ) : (
        <>
          <polygon
            points={`0,${SPARKLINE_HEIGHT} ${points} ${SPARKLINE_WIDTH},${SPARKLINE_HEIGHT}`}
            className="asb-gateway-sparkline-area"
          />
          <polyline points={points} className="asb-gateway-sparkline-line" />
        </>
      )}
    </svg>
  );
}

function buildMinuteBuckets(samples: GatewaySample[]): number[] {
  const startMinute = Math.floor(Date.now() / MINUTE_MS) - (TREND_MINUTES - 1);
  const buckets = new Array<number>(TREND_MINUTES).fill(0);
  for (const sample of samples) {
    const index = Math.floor(sample.atMs / MINUTE_MS) - startMinute;
    if (index >= 0 && index < TREND_MINUTES) buckets[index] += 1;
  }
  return buckets;
}

function buildProtocolUsage(samples: GatewaySample[]): Array<{ protocol: UpstreamProtocol; count: number }> {
  const counts = new Map<UpstreamProtocol, number>();
  for (const sample of samples) {
    if (sample.upstreamProtocol !== null) {
      counts.set(sample.upstreamProtocol, (counts.get(sample.upstreamProtocol) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .map(([protocol, count]) => ({ protocol, count }))
    .sort((a, b) => b.count - a.count);
}

function percentile(sorted: number[], p: number): number {
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(sorted.length - 1, Math.max(0, index))];
}

function formatDuration(durationMs: number): string {
  return durationMs < 1000 ? `${durationMs} ms` : `${(durationMs / 1000).toFixed(1)} s`;
}

function GatewayRecentRequests({ samples, profileNames }: {
  samples: GatewaySample[];
  profileNames: Map<string, string>;
}) {
  const [open, setOpen] = useState(false);
  const recent = [...samples].slice(-RECENT_ROW_COUNT).reverse();
  return (
    <section className="asb-gateway-recent" aria-label="最近请求">
      <button
        type="button"
        className="asb-gateway-recent-toggle"
        aria-expanded={open}
        aria-controls="asb-gateway-recent-body"
        onClick={() => setOpen((value) => !value)}
      >
        <span aria-hidden="true" className="asb-gateway-recent-chevron"><ChevronDownIcon /></span>
        最近请求
        <span className="asb-gateway-recent-count">
          {samples.length > 0 ? `最近 ${recent.length} 条` : "暂无记录"}
        </span>
      </button>
      {open && (
        <div id="asb-gateway-recent-body" className="asb-gateway-recent-body">
          {recent.length === 0 ? (
            <p className="asb-gateway-empty" role="status">暂无网关请求记录。</p>
          ) : (
            <div className="asb-gateway-recent-table">
              <Table
                ariaLabel="最近经本机网关处理的请求"
                columns={recentColumns(profileNames)}
                rows={recent}
                rowKey={(sample) =>
                  `${sample.atMs}:${sample.app}:${sample.profileId ?? "-"}:${sample.routeRevision ?? "-"}`
                }
              />
            </div>
          )}
          <p className="asb-gateway-note">
            遥测仅保存在内存中，重启应用后清空；缓冲区满时只保留最近 512 条请求。
          </p>
        </div>
      )}
    </section>
  );
}

function recentColumns(profileNames: Map<string, string>): Array<TableColumn<GatewaySample>> {
  return [
    {
      key: "time",
      header: "时间",
      render: (sample) => <span className="asb-gateway-row-value asb-code">{formatClock(sample.atMs)}</span>,
    },
    { key: "app", header: "客户端", render: (sample) => APP_LABELS[sample.app] },
    {
      key: "profile",
      header: "供应商",
      render: (sample) =>
        sample.profileId === null
          ? "未匹配路由"
          : profileNames.get(sample.profileId) ?? "已删除的供应商",
    },
    {
      key: "routeRevision",
      header: "路由修订",
      render: (sample) =>
        sample.routeRevision === null ? (
          "—"
        ) : (
          <span className="asb-gateway-revision asb-code">{sample.routeRevision}</span>
        ),
    },
    {
      key: "path",
      header: "路径",
      render: (sample) => <GatewayRequestPath sample={sample} />,
    },
    {
      key: "status",
      header: "状态",
      render: (sample) => {
        if (sample.status === null) return <span className="asb-fail-text">中断</span>;
        if (sample.status >= 400) return <span className="asb-fail-text">HTTP {sample.status}</span>;
        return <span className="asb-gateway-row-value">{sample.status}</span>;
      },
    },
    {
      key: "duration",
      header: "耗时",
      render: (sample) => (
        <span className="asb-gateway-row-value">{formatDuration(sample.durationMs)}</span>
      ),
    },
  ];
}

function GatewayRequestPath({ sample }: { sample: GatewaySample }) {
  if (sample.upstreamProtocol === null) return "未匹配路由";
  const translated = isProtocolTranslation(sample.clientProtocol, sample.upstreamProtocol);
  return (
    <span className="asb-gateway-request-path">
      <span>{PROTOCOL_LABELS[sample.clientProtocol]}</span>
      <span aria-hidden="true">→</span>
      <span>{PROTOCOL_LABELS[sample.upstreamProtocol]}</span>
      <span className={`asb-gateway-request-mode is-${translated ? "translation" : "relay"}`}>
        {translated ? "协议转换" : "本机转发"}
      </span>
    </span>
  );
}
