import { useEffect, useState } from "react";
import * as api from "../../api/codex-endpoints";
import type { CodexProviderRecord } from "../../api/providers";
import { Button } from "../Button";
import { Input } from "../Input";
import { Table } from "../Table";
import type { CodexOperations } from "./operations";
/** Extra upstream targets for one Codex provider. Routing tries the primary first, then
 * recently used custom targets; the race below only measures, it never reorders. */
export function CodexEndpointsPanel({ record, operations: { run, busy, changed }, onClose }: { record: CodexProviderRecord; operations: CodexOperations; onClose: () => void }) {
  const [view, setView] = useState<api.CodexEndpointsView | null>(null);
  const [url, setUrl] = useState("");
  const [latency, setLatency] = useState<api.CodexEndpointLatency[] | null>(null);
  useEffect(() => { void run(async () => setView(await api.listCodexEndpoints(record.profile.id))); }, [run, record.profile.id]);
  if (!view) return <p role="status">正在读取 Codex 服务端点…</p>;
  const latencyOf = (target: string) => latency?.find((row) => row.url === target);
  const describe = (row: api.CodexEndpointLatency | undefined) => !row ? "—" : row.error ? `不可达：${row.error}` : `${row.latencyMs ?? "?"} ms · HTTP ${row.status ?? "?"}`;
  return <section className="asb-provider-section-fields" aria-label="Codex 服务端点">
    <h4 className="asb-group-title">{record.profile.name} · 服务端点</h4>
    <p className="asb-scope-note">主端点来自供应商编辑器，此处只管理备用地址。请求先走主端点，失败后按最近成功顺序尝试备用地址；测速不发送凭据，也不会自动改排序。正在使用中的供应商需重新应用后才能修改。</p>
    <p>主端点：<code className="asb-code">{view.primary}</code>{latency && <span> · {describe(latencyOf(view.primary))}</span>}</p>
    <Table ariaLabel="Codex 备用端点" rows={view.endpoints} rowKey={(endpoint) => endpoint.url} columns={[
      { key: "url", header: "地址", render: (endpoint) => endpoint.url },
      { key: "used", header: "最近成功", render: (endpoint) => endpoint.lastUsed ? new Date(endpoint.lastUsed).toLocaleString() : "尚未使用" },
      { key: "latency", header: "测速", render: (endpoint) => describe(latencyOf(endpoint.url)) },
      { key: "remove", header: "操作", render: (endpoint) => <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
        setView(await api.removeCodexEndpoint(view.providerId, endpoint.url, view.fileHash, true)); setLatency(null); changed("已删除 Codex 备用端点。");
      })}>删除 {endpoint.url}</Button> },
    ]} />
    <form className="asb-form-actions" onSubmit={(e) => { e.preventDefault(); void run(async () => {
      setView(await api.addCodexEndpoint(view.providerId, url.trim(), view.fileHash, true)); setUrl(""); setLatency(null); changed("已添加 Codex 备用端点，尚未参与路由；重新应用供应商后生效。");
    }); }}>
      <Input aria-label="新增 Codex 备用端点" value={url} placeholder="https://backup.example/v1" disabled={busy} onChange={(e) => setUrl(e.target.value)} />
      <Button type="submit" variant="secondary" disabled={busy || !url.trim()}>添加备用端点</Button>
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setLatency(await api.testCodexEndpoints([view.primary, ...view.endpoints.map((endpoint) => endpoint.url)])))}>测速全部端点</Button>
      <Button variant="secondary" disabled={busy} onClick={onClose}>返回供应商列表</Button>
    </form>
  </section>;
}
