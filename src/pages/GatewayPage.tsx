import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Button } from "../components/Button";
import { Table, type TableColumn } from "../components/Table";
import { PROTOCOL_LABELS } from "../lib/protocol";
import {
  getGatewayStatus,
  type AppKind,
  type GatewaySample,
  type GatewayStatus,
  type ProviderProfile,
  type UpstreamProtocol,
} from "../api/client";

const POLL_INTERVAL_MS = 5000;
const TREND_MINUTES = 60;
const RECENT_ROW_COUNT = 20;
const MINUTE_MS = 60_000;

const APP_LABELS: Record<AppKind, string> = { codex: "Codex", claude: "Claude Code" };

/** Fixed protocol → series identity, independent of how many appear. */
const PROTOCOL_TONES: Record<UpstreamProtocol, number> = {
  responses: 1,
  chatCompletions: 2,
  anthropicMessages: 3,
};

interface Props {
  /** False while another page is shown; polling runs only when visible. */
  active: boolean;
  profiles: ProviderProfile[];
}

/** 网关页：本机协议网关的监听状态、活动路由与请求遥测。全部事实来自后端
 * `gateway_status` 快照；本页不做任何配置写入。 */
export function GatewayPage({ active, profiles }: Props) {
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  if (error) {
    return (
      <GatewayPanel>
        <p className="m-0 text-body-medium text-text-error-primary" role="alert">
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
        <p className="m-0 text-body-medium text-text-tertiary" role="status">
          正在读取网关状态…
        </p>
      </GatewayPanel>
    );
  }

  const nowMs = Date.now();
  const trend = buildTrendRows(status.metrics.samples, nowMs);
  const protocolUsage = buildProtocolUsage(status.metrics.samples);
  const recent = [...status.metrics.samples].slice(-RECENT_ROW_COUNT).reverse();

  return (
    <GatewayPanel>
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <p className="m-0 text-body-medium text-text-secondary">
          本机监听 <span className="tabular-nums">127.0.0.1:{status.port}</span>
          ，仅当供应商上游协议与客户端原生协议不同时，请求才经此转换。
        </p>
        <Button variant="secondary" disabled={!active} onClick={() => void refresh()}>
          刷新
        </Button>
      </div>

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <StatTile label="累计请求" value={status.metrics.totalRequests} />
        <StatTile label="失败请求" value={status.metrics.failedRequests} />
        <StatTile label="活动路由" value={status.routes.length} />
        <StatTile label="监听端口" value={status.port} />
      </div>

      <section aria-label="活动路由" className="flex flex-col gap-2">
        <h3 className="m-0 text-title-3-semibold text-text-primary">活动路由</h3>
        {status.routes.length === 0 ? (
          <p className="m-0 text-body-medium text-text-tertiary" role="status">
            当前没有经本机协议网关转换的供应商；客户端均在直连或官方登录。
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {status.routes.map((route) => (
              <li
                key={`${route.app}:${route.profileId}`}
                className="flex flex-wrap items-center gap-2 text-body-medium text-text-secondary"
              >
                <span className="text-text-primary">{APP_LABELS[route.app]}</span>
                <span>{profileNames.get(route.profileId) ?? "已删除的供应商"}</span>
                <span className="text-text-tertiary">
                  上游 {PROTOCOL_LABELS[route.upstreamProtocol]}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <TrendCard rows={trend} />

      <section aria-label="上游协议分布" className="flex flex-col gap-3">
        <h3 className="m-0 text-title-3-semibold text-text-primary">上游协议分布</h3>
        {protocolUsage.length === 0 ? (
          <p className="m-0 text-body-medium text-text-tertiary" role="status">
            暂无网关请求记录。
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {protocolUsage.map((entry) => (
              <li key={entry.protocol} className="flex items-center gap-3 text-body-medium">
                <span className="w-36 shrink-0 text-text-secondary">
                  {PROTOCOL_LABELS[entry.protocol]}
                </span>
                <span
                  className="h-2 min-w-0 rounded-full"
                  style={{
                    width: `${Math.max((entry.count / entry.max) * 100, 2)}%`,
                    backgroundColor: `var(--color-chart-${PROTOCOL_TONES[entry.protocol]})`,
                  }}
                  aria-hidden
                />
                <span className="shrink-0 text-text-primary tabular-nums">{entry.count}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-label="最近请求" className="flex flex-col gap-3">
        <h3 className="m-0 text-title-3-semibold text-text-primary">最近请求</h3>
        {recent.length === 0 ? (
          <p className="m-0 text-body-medium text-text-tertiary" role="status">
            暂无网关请求记录。
          </p>
        ) : (
          <Table
            ariaLabel="最近经本机协议网关转换的请求"
            columns={recentColumns(profileNames)}
            rows={recent}
            rowKey={(sample) => `${sample.atMs}:${sample.app}:${sample.profileId ?? "-"}`}
          />
        )}
        <p className="m-0 text-body-2-medium text-text-tertiary">
          遥测仅保存在内存中，重启应用后清空；缓冲区满时图表只保留最近 512 条请求。
        </p>
      </section>
    </GatewayPanel>
  );
}

function GatewayPanel({ children }: { children: ReactNode }) {
  return (
    <section className="asb-panel" aria-label="协议网关">
      <div className="asb-panel-heading">
        <h2 className="asb-panel-title">协议网关</h2>
      </div>
      <div className="flex flex-col gap-6">{children}</div>
    </section>
  );
}

function StatTile({ label, value }: { label: string; value: number }) {
  return (
    <div
      role="group"
      aria-label={label}
      className="flex flex-col gap-1 rounded-2xl bg-background-secondary-default px-4 py-3"
    >
      <p className="m-0 text-body-2-medium text-text-secondary">{label}</p>
      <p className="m-0 text-title-2-medium text-text-primary tabular-nums">{value}</p>
    </div>
  );
}

interface TrendRow {
  timestamp: number;
  codex: number | undefined;
  claude: number | undefined;
}

/** Buckets the last hour into per-minute counts. Minutes before the first
 * recorded request have no observation and stay undefined; minutes after it
 * are real zeros, not gaps. */
function buildTrendRows(samples: GatewaySample[], nowMs: number): TrendRow[] {
  const currentMinute = Math.floor(nowMs / MINUTE_MS);
  const startMinute = currentMinute - (TREND_MINUTES - 1);
  const rows: TrendRow[] = Array.from({ length: TREND_MINUTES }, (_, offset) => ({
    timestamp: (startMinute + offset) * MINUTE_MS,
    codex: undefined,
    claude: undefined,
  }));
  let firstMinute = Number.MAX_SAFE_INTEGER;
  for (const sample of samples) firstMinute = Math.min(firstMinute, Math.floor(sample.atMs / MINUTE_MS));
  for (const sample of samples) {
    const index = Math.floor(sample.atMs / MINUTE_MS) - startMinute;
    if (index < 0 || index >= TREND_MINUTES) continue;
    rows[index][sample.app] = (rows[index][sample.app] ?? 0) + 1;
  }
  for (const row of rows) {
    if (row.timestamp / MINUTE_MS < firstMinute) continue;
    row.codex ??= 0;
    row.claude ??= 0;
  }
  return rows;
}

function buildProtocolUsage(samples: GatewaySample[]) {
  const counts = new Map<UpstreamProtocol, number>();
  for (const sample of samples) {
    const protocol = sample.upstreamProtocol;
    if (protocol === null) continue;
    counts.set(protocol, (counts.get(protocol) ?? 0) + 1);
  }
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  const max = entries[0]?.[1] ?? 1;
  return entries.map(([protocol, count]) => ({ protocol, count, max }));
}

function recentColumns(profileNames: Map<string, string>): Array<TableColumn<GatewaySample>> {
  return [
    {
      key: "time",
      header: "时间",
      render: (sample) => <span className="tabular-nums">{formatClock(sample.atMs)}</span>,
    },
    { key: "app", header: "客户端", render: (sample) => APP_LABELS[sample.app] },
    {
      key: "profile",
      header: "供应商",
      render: (sample) =>
        sample.profileId === null ? "未匹配路由" : profileNames.get(sample.profileId) ?? "已删除的供应商",
    },
    {
      key: "protocol",
      header: "上游协议",
      render: (sample) =>
        sample.upstreamProtocol === null ? "—" : PROTOCOL_LABELS[sample.upstreamProtocol],
    },
    {
      key: "status",
      header: "状态",
      render: (sample) => {
        if (sample.status === null) return <span className="text-text-error-primary">中断</span>;
        if (sample.status >= 400)
          return <span className="text-text-error-primary">HTTP {sample.status}</span>;
        return <span className="tabular-nums">{sample.status}</span>;
      },
    },
    {
      key: "duration",
      header: "耗时",
      render: (sample) => <span className="tabular-nums">{formatDuration(sample.durationMs)}</span>,
    },
  ];
}

function TrendCard({ rows }: { rows: TrendRow[] }) {
  const total = rows.reduce((sum, row) => sum + (row.codex ?? 0) + (row.claude ?? 0), 0);
  return (
    <figure
      className="bui-scope flex h-72 w-full min-w-0 flex-col gap-4 rounded-2xl bg-background-secondary-default px-4 pt-4 pb-2"
      aria-label="近 60 分钟请求趋势"
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-0.5">
          <figcaption className="m-0 text-title-3-semibold text-text-primary">
            近 60 分钟请求
          </figcaption>
          <p className="m-0 text-body-2-medium text-text-tertiary tabular-nums">
            共 {total} 次请求
          </p>
        </div>
        <dl className="m-0 flex shrink-0 items-center gap-4 text-body-2-medium text-text-secondary">
          {(["codex", "claude"] as const).map((series, index) => (
            <div key={series} className="flex items-center gap-1.5">
              <span
                className="size-2 rounded-full"
                style={{ backgroundColor: `var(--color-chart-${index + 1})` }}
                aria-hidden
              />
              <dt className="whitespace-nowrap">{APP_LABELS[series]}</dt>
            </div>
          ))}
        </dl>
      </div>
      <div className="min-h-0 w-full flex-1">
        <ResponsiveContainer width="100%" height="100%">
          <ComposedChart data={rows} margin={{ top: 4, right: 6, bottom: 0, left: 0 }}>
            <XAxis
              dataKey="timestamp"
              type="number"
              scale="time"
              domain={["dataMin", "dataMax"]}
              tickFormatter={(value: number) => formatClock(Number(value))}
              tickLine={false}
              axisLine={false}
              tickMargin={12}
              tick={{ fontSize: 12, fill: "var(--color-text-tertiary)" }}
            />
            <YAxis
              width={44}
              allowDecimals={false}
              tickLine={false}
              axisLine={false}
              tick={{ fontSize: 12, fill: "var(--color-text-tertiary)" }}
            />
            <Tooltip content={<TrendTooltip />} cursor={{ stroke: "var(--color-chart-cursor)", strokeWidth: 1, strokeDasharray: "4 4" }} />
            <Line
              type="monotone"
              dataKey="codex"
              name="Codex"
              stroke="var(--color-chart-1)"
              strokeWidth={2}
              dot={false}
              connectNulls
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="claude"
              name="Claude Code"
              stroke="var(--color-chart-2)"
              strokeWidth={2}
              dot={false}
              connectNulls
              isAnimationActive={false}
            />
          </ComposedChart>
        </ResponsiveContainer>
      </div>
    </figure>
  );
}

function TrendTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: Array<{ dataKey: string; value?: number | string }>;
  label?: number | string;
}) {
  if (!active || !payload?.length) return null;
  const parts = payload
    .filter((entry) => typeof entry.value === "number")
    .map((entry) => `${entry.dataKey === "codex" ? "Codex" : "Claude Code"}：${entry.value}`);
  return (
    <div
      className="rounded-lg px-3 py-2 text-body-2-medium text-text-primary shadow-md"
      style={{ background: "var(--asb-chart-tooltip-bg)" }}
    >
      <p className="m-0 tabular-nums">{formatClock(Number(label))}</p>
      {parts.map((part) => (
        <p key={part} className="m-0 tabular-nums">
          {part}
        </p>
      ))}
    </div>
  );
}

function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function formatDuration(durationMs: number): string {
  if (durationMs < 1000) return `${durationMs} ms`;
  return `${(durationMs / 1000).toFixed(1)} s`;
}
