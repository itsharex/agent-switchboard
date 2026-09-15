import { useEffect, useState } from "react";
import { addProviderEndpoint, listProviderEndpoints, removeProviderEndpoint, type ProviderEndpointsView, type ProviderRecord } from "../../api/providers";
import { testClaudeEndpoints, type ClaudeEndpointLatency } from "../../api/claude-providers";
import { Button } from "../Button";
import { Input } from "../Input";
import { Table } from "../Table";
import type { ClaudeOperations } from "./operations";
/** Candidate upstream endpoints of one custom Claude profile: list, add,
 * remove, and a credential-free batch latency test. */
export function EndpointsPanel({ record, operations: op }: { record: ProviderRecord; operations: ClaudeOperations }) {
  const [view, setView] = useState<ProviderEndpointsView | null>(null);
  const [url, setUrl] = useState("");
  const [latency, setLatency] = useState<Record<string, ClaudeEndpointLatency>>({});
  const { run, busy, changed } = op;
  const id = record.profile.id;
  useEffect(() => { void run(async () => setView(await listProviderEndpoints(id))); }, [run, id]);
  if (!view) return null;
  const urls = [record.profile.baseUrl, ...view.endpoints.map((endpoint) => endpoint.url)].filter((value): value is string => !!value);
  const cell = (target: string) => {
    const probe = latency[target]; if (!probe) return "未测试";
    if (probe.error) return probe.error;
    const result = probe.result!; return (result.grade === "ok" ? "正常" : result.grade === "slow" ? "较慢" : "不可达") + (result.latencyMs === null ? "" : ` · ${result.latencyMs} ms`) + (result.status === null ? "" : ` · HTTP ${result.status}`);
  };
  return <section aria-label="Claude 候选端点" className="asb-provider-section-fields">
    <h3 className="asb-section-title">候选端点</h3>
    <p className="asb-scope-note">候选端点由网关在启用「自动选择」时按最近成功记录切换；测速只做无凭据的可达性探测，不发送模型请求。当前生效档案需重新应用后才能改动端点。</p>
    <Table ariaLabel="Claude 候选端点列表" rows={urls} rowKey={(value) => value} columns={[
      { key: "url", header: "地址", render: (value) => value + (value === record.profile.baseUrl ? "（主地址）" : "") },
      { key: "latency", header: "测速", render: cell },
      { key: "actions", header: "操作", render: (value) => value === record.profile.baseUrl ? null
        : <Button variant="danger" disabled={busy} onClick={() => void run(async () => { setView(await removeProviderEndpoint(id, value, view.fileHash, true)); changed("Claude 候选端点已删除。"); })}>删除端点</Button> },
    ]} />
    <div className="asb-form-actions">
      <Input aria-label="新增候选端点地址" type="url" value={url} disabled={busy} placeholder="https://relay.example/v1" onChange={(e) => setUrl(e.target.value)} />
      <Button variant="secondary" disabled={busy || !url.trim()} onClick={() => void run(async () => { setView(await addProviderEndpoint(id, url.trim(), view.fileHash, true)); setUrl(""); changed("Claude 候选端点已添加。"); })}>添加候选端点</Button>
      <Button variant="secondary" disabled={busy || urls.length === 0} onClick={() => void run(async () => {
        const results = await testClaudeEndpoints(urls); setLatency(Object.fromEntries(results.map((result) => [result.url, result])));
      })}>测试全部端点延迟</Button>
    </div>
  </section>;
}
