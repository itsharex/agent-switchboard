import { useEffect, useState } from "react";
import * as api from "../../api/claude-ledger";
import { searchClaudeProfiles } from "../../api/claude-providers";
import type { ProviderRecord } from "../../api/providers";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { CodePreview } from "../CodePreview";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table } from "../Table";
import { PriceEditor } from "./PriceEditor";
import type { ClaudeOperations } from "./operations";
function Filters({ value, change, busy }: { value: api.ClaudeLedgerFilter; change: (filter: api.ClaudeLedgerFilter) => void; busy: boolean }) {
  return <div className="asb-provider-field-grid">
    <label className="asb-field"><span>计量模型筛选</span><Input value={value.model ?? ""} disabled={busy} onChange={(e) => change({ ...value, model: e.target.value || null })} /></label>
    {([['from', '起始时间'], ['to', '结束时间']] as const).map(([key, label]) => <label className="asb-field" key={key}><span>{label}</span><Input type="datetime-local" value={value[key] ?? ""} disabled={busy} onChange={(e) => change({ ...value, [key]: e.target.value || null })} /></label>)}
    <Checkbox label="仅显示失败的 Claude 请求" checked={value.failuresOnly ?? false} disabled={busy} onChange={(failuresOnly) => change({ ...value, failuresOnly })} />
  </div>;
}
function resolved(filter: api.ClaudeLedgerFilter): api.ClaudeLedgerFilter {
  const time = (value?: string | null) => value ? new Date(value).toISOString() : null;
  return { ...filter, from: time(filter.from), to: time(filter.to) };
}
function Summary({ value }: { value: api.ClaudeRequestLedgerSummary }) {
  return <section aria-label="Claude 请求计量汇总"><p>请求 {value.totalRequests} · 失败 {value.failedRequests}</p>
    <p>输入 {value.inputTokens ?? "未知"} · 输出 {value.outputTokens ?? "未知"} · 缓存读取 {value.cacheReadTokens ?? "未知"} · 缓存写入 {value.cacheCreationTokens ?? "未知"}</p>
    <p>已知部分费用 USD {value.estimatedCostUsd} · 已计价 {value.pricedRequests} · 未计价 {value.unpricedRequests}</p>
    <p>平均首 token {value.averageFirstTokenLatencyMs === null ? "未知" : value.averageFirstTokenLatencyMs + " ms"} · 平均请求 {value.averageDurationMs === null ? "未知" : value.averageDurationMs + " ms"}</p>
  </section>;
}
export function MeteringPane({ operations: op }: { operations: ClaudeOperations }) {
  const [filter, setFilter] = useState<api.ClaudeLedgerFilter>({});
  const [applied, setApplied] = useState<api.ClaudeLedgerFilter>({});
  const [profiles, setProfiles] = useState<ProviderRecord[]>([]);
  const [page, setPage] = useState<api.ClaudeRequestLedgerPage | null>(null);
  const [summary, setSummary] = useState<api.ClaudeRequestLedgerSummary | null>(null);
  const [book, setBook] = useState<api.ClaudePriceBookSnapshot | null>(null);
  const [detail, setDetail] = useState<api.ClaudeRequestRecord | null>(null);
  const { run, busy } = op;
  useEffect(() => { void run(async () => {
    const [entries, total, prices, records] = await Promise.all([api.getClaudeRequestLedger(), api.getClaudeRequestLedgerSummary(), api.getClaudePriceBook(), searchClaudeProfiles("")]);
    setPage(entries); setSummary(total); setBook(prices); setProfiles(records);
  }); }, [run]);
  const query = (offset: number, criteria: api.ClaudeLedgerFilter) => void run(async () => {
    const [entries, total] = await Promise.all([api.getClaudeRequestLedger(offset, 50, criteria), api.getClaudeRequestLedgerSummary(criteria)]);
    setPage(entries); setSummary(total); setApplied(criteria); setDetail(null);
  });
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">这里是持久化的 Claude 网关请求账本，不是 CLI 会话用量或托管订阅额度。未知价格与 token 单独标记，不按零计费。</p>
    <Select ariaLabel="Claude 计量供应商" value={filter.profileId ?? "all"} disabled={busy} onChange={(id) => setFilter({ ...filter, profileId: id === "all" ? null : id })}
      options={[{ value: "all", label: "全部 Claude 供应商" }, ...profiles.map((r) => ({ value: r.profile.id, label: r.profile.name }))]} />
    <Filters value={filter} change={setFilter} busy={busy} />
    <Button variant="secondary" disabled={busy} onClick={() => { try { query(0, resolved(filter)); } catch { op.setError("计量筛选时间无效"); } }}>查询 Claude 请求</Button>
    {summary && <Summary value={summary} />}
    {page && <>
      <Table ariaLabel="Claude 请求账本" rows={page.entries} rowKey={(_r, index) => String(page.offset + index)} columns={[
        { key: "at", header: "时间", render: (e) => new Date(e.at).toLocaleString() },
        { key: "profile", header: "供应商", render: (e) => profiles.find((p) => p.profile.id === e.profileId)?.profile.name ?? e.profileId ?? "未知" },
        { key: "model", header: "实际模型", render: (e) => e.responseModel ?? "未返回" },
        { key: "usage", header: "输入 / 输出", render: (e) => (e.inputTokens ?? "未知") + " / " + (e.outputTokens ?? "未知") },
        { key: "cost", header: "费用 USD", render: (e) => e.cost?.totalUsd ?? "未计价" },
        { key: "details", header: "结果", render: (e) => <Button variant="secondary" onClick={() => setDetail(e)}>{e.status ?? "未完成"} · 详情</Button> },
      ]} />
      {page.entries.length === 0 && <p className="asb-scope-note">此筛选下没有 Claude 请求</p>}
      <div className="asb-form-actions"><Button variant="secondary" disabled={busy || page.offset === 0} onClick={() => query(Math.max(0, page.offset - page.limit), applied)}>上一页</Button>
        <span>{page.total} 条请求</span><Button variant="secondary" disabled={busy || !page.hasMore} onClick={() => query(page.offset + page.limit, applied)}>下一页</Button></div>
    </>}
    {detail && <CodePreview target="Claude 请求详情（含映射模型、Failover 尝试与计价来源）" content={JSON.stringify(detail, null, 2)} />}
    {book && <PriceEditor snapshot={book} operations={op} onSaved={setBook} />}
  </div>;
}
