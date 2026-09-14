import type { GatewaySample, UpstreamProtocol } from "../api/client";
import { PROTOCOL_LABELS } from "../lib/protocol";
import { GatewayTrendChart, formatClock } from "./charts/GatewayTrendChart";
import { Table, type TableColumn } from "./Table";

const RECENT_ROW_COUNT = 20;
const APP_LABELS = { codex: "Codex", claude: "Claude Code" } as const;
const PROTOCOL_TONES: Record<UpstreamProtocol, number> = {
  responses: 1,
  chatCompletions: 2,
  anthropicMessages: 3,
  geminiGenerateContent: 4,
};

export function GatewayTelemetry({
  samples,
  profileNames,
}: {
  samples: GatewaySample[];
  profileNames: Map<string, string>;
}) {
  const protocolUsage = buildProtocolUsage(samples);
  const recent = [...samples].slice(-RECENT_ROW_COUNT).reverse();
  return (
    <>
      <GatewayTrendChart samples={samples} />
      <section aria-label="上游协议分布" className="asb-gateway-section">
        <h3 className="asb-section-title">上游协议分布</h3>
        {protocolUsage.length === 0 ? (
          <p className="asb-gateway-empty" role="status">暂无网关请求记录。</p>
        ) : (
          <ul className="asb-gateway-protocol-list">
            {protocolUsage.map((entry) => (
              <li key={entry.protocol}>
                <span className="asb-gateway-protocol-name">
                  {PROTOCOL_LABELS[entry.protocol]}
                </span>
                <span
                  className="asb-gateway-protocol-bar"
                  style={{
                    width: `${Math.max((entry.count / entry.max) * 100, 2)}%`,
                    backgroundColor: `var(--color-chart-${PROTOCOL_TONES[entry.protocol]})`,
                  }}
                  aria-hidden
                />
                <span className="asb-gateway-protocol-count">{entry.count}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
      <section aria-label="最近请求" className="asb-gateway-section">
        <h3 className="asb-section-title">最近请求</h3>
        {recent.length === 0 ? (
          <p className="asb-gateway-empty" role="status">暂无网关请求记录。</p>
        ) : (
          <Table
            ariaLabel="最近经本机协议网关转换的请求"
            columns={recentColumns(profileNames)}
            rows={recent}
            rowKey={(sample) =>
              `${sample.atMs}:${sample.app}:${sample.profileId ?? "-"}:${sample.routeRevision ?? "-"}`
            }
          />
        )}
        <p className="asb-gateway-note">
          遥测仅保存在内存中，重启应用后清空；缓冲区满时图表只保留最近 512 条请求。
        </p>
      </section>
    </>
  );
}

function buildProtocolUsage(samples: GatewaySample[]) {
  const counts = new Map<UpstreamProtocol, number>();
  for (const sample of samples) {
    if (sample.upstreamProtocol !== null) {
      counts.set(sample.upstreamProtocol, (counts.get(sample.upstreamProtocol) ?? 0) + 1);
    }
  }
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  const max = entries[0]?.[1] ?? 1;
  return entries.map(([protocol, count]) => ({ protocol, count, max }));
}

function recentColumns(profileNames: Map<string, string>): Array<TableColumn<GatewaySample>> {
  return [
    { key: "time", header: "时间", render: (sample) => <span className="asb-gateway-row-value asb-code">{formatClock(sample.atMs)}</span> },
    { key: "app", header: "客户端", render: (sample) => APP_LABELS[sample.app] },
    {
      key: "profile",
      header: "供应商",
      render: (sample) => sample.profileId === null ? "未匹配路由" : profileNames.get(sample.profileId) ?? "已删除的供应商",
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
      key: "protocol",
      header: "上游协议",
      render: (sample) => sample.upstreamProtocol === null ? "—" : PROTOCOL_LABELS[sample.upstreamProtocol],
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
    { key: "duration", header: "耗时", render: (sample) => <span className="asb-gateway-row-value">{formatDuration(sample.durationMs)}</span> },
  ];
}

function formatDuration(durationMs: number): string {
  return durationMs < 1000 ? `${durationMs} ms` : `${(durationMs / 1000).toFixed(1)} s`;
}
