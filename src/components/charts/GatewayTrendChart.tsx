import {
  ComposedChart,
  Line,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { GatewaySample } from "../../api/client";
import { ChartFrame } from "./ChartFrame";

const TREND_MINUTES = 60;
const MINUTE_MS = 60_000;
const APP_LABELS = { codex: "Codex", claude: "Claude Code" } as const;

export function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

interface TrendRow {
  timestamp: number;
  codex: number | undefined;
  claude: number | undefined;
}

function buildTrendRows(samples: GatewaySample[], nowMs: number): TrendRow[] {
  const currentMinute = Math.floor(nowMs / MINUTE_MS);
  const startMinute = currentMinute - (TREND_MINUTES - 1);
  const rows: TrendRow[] = Array.from({ length: TREND_MINUTES }, (_, offset) => ({
    timestamp: (startMinute + offset) * MINUTE_MS,
    codex: undefined,
    claude: undefined,
  }));
  let firstMinute = Number.MAX_SAFE_INTEGER;
  for (const sample of samples) {
    firstMinute = Math.min(firstMinute, Math.floor(sample.atMs / MINUTE_MS));
  }
  for (const sample of samples) {
    const index = Math.floor(sample.atMs / MINUTE_MS) - startMinute;
    if (index >= 0 && index < TREND_MINUTES) {
      rows[index][sample.app] = (rows[index][sample.app] ?? 0) + 1;
    }
  }
  for (const row of rows) {
    if (row.timestamp / MINUTE_MS >= firstMinute) {
      row.codex ??= 0;
      row.claude ??= 0;
    }
  }
  return rows;
}

/** 近 60 分钟网关请求趋势复用网关页的材料、文字和状态色，
 * 而不是引入图表专属的背景或阴影。 */
export function GatewayTrendChart({ samples }: { samples: GatewaySample[] }) {
  const rows = buildTrendRows(samples, Date.now());
  const total = rows.reduce(
    (sum, row) => sum + (row.codex ?? 0) + (row.claude ?? 0),
    0,
  );
  return (
    <figure className="asb-gateway-trend" aria-label="近 60 分钟请求趋势">
      <GatewayTrendHeader total={total} />
      {total === 0 ? (
        <p className="asb-gateway-trend-empty" role="status">
          暂无网关请求记录。
        </p>
      ) : (
        <GatewayTrendPlot rows={rows} />
      )}
    </figure>
  );
}

function GatewayTrendHeader({ total }: { total: number }) {
  return (
    <figcaption className="asb-gateway-trend-header">
      <span className="asb-gateway-trend-copy">
        <span className="asb-gateway-trend-title">近 60 分钟请求</span>
        <span className="asb-gateway-trend-summary">共 {total} 次请求</span>
      </span>
      <span className="asb-gateway-trend-legend" role="list" aria-label="请求来源">
        {(["codex", "claude"] as const).map((series, index) => (
          <span
            key={series}
            className="asb-gateway-trend-legend-entry"
            role="listitem"
          >
            <span
              className="asb-gateway-trend-legend-dot"
              style={{ backgroundColor: `var(--color-chart-${index + 1})` }}
              aria-hidden
            />
            {APP_LABELS[series]}
          </span>
        ))}
      </span>
    </figcaption>
  );
}

function GatewayTrendPlot({ rows }: { rows: TrendRow[] }) {
  const ticks = rows
    .filter((_, index) => index % 10 === 0)
    .map((row) => row.timestamp);
  return (
    <div className="asb-gateway-trend-chart">
      <ChartFrame>
        <ComposedChart data={rows} margin={{ top: 4, right: 6, bottom: 0, left: 0 }}>
          <XAxis
            dataKey="timestamp"
            type="number"
            scale="time"
            domain={["dataMin", "dataMax"]}
            ticks={ticks}
            tickFormatter={(value: number) => formatClock(Number(value))}
            tickLine={false}
            axisLine={false}
            tickMargin={12}
            tick={{ fontSize: 12, fill: "var(--asb-text-muted)" }}
          />
          <YAxis
            width={44}
            allowDecimals={false}
            tickLine={false}
            axisLine={false}
            tick={{ fontSize: 12, fill: "var(--asb-text-muted)" }}
          />
          <Tooltip
            content={<TrendTooltip />}
            cursor={{
              stroke: "var(--asb-hairline-strong)",
              strokeWidth: 1,
              strokeDasharray: "4 4",
            }}
          />
          <Line
            type="monotone"
            dataKey="codex"
            name="Codex"
            stroke="var(--asb-action)"
            strokeWidth={2}
            dot={false}
            connectNulls
            isAnimationActive={false}
          />
          <Line
            type="monotone"
            dataKey="claude"
            name="Claude Code"
            stroke="var(--asb-claude)"
            strokeWidth={2}
            dot={false}
            connectNulls
            isAnimationActive={false}
          />
        </ComposedChart>
      </ChartFrame>
    </div>
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
    .map(
      (entry) =>
        `${entry.dataKey === "codex" ? "Codex" : "Claude Code"}：${entry.value}`,
    );
  return (
    <div className="asb-gateway-trend-tooltip">
      <p>{formatClock(Number(label))}</p>
      {parts.map((part) => (
        <p key={part}>{part}</p>
      ))}
    </div>
  );
}