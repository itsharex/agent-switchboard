import { useEffect, useState } from "react";
import * as api from "../../api/codex-metering";
import { listCodexProfiles } from "../../api/providers";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table } from "../Table";
import { PriceEditor, BillingEditor } from "./PriceEditor";
import type { CodexOperations } from "./operations";
export function MeteringPane({ operations: { run, busy, changed } }: { operations: CodexOperations }) {
  const [snapshot, setSnapshot] = useState<api.CodexMeteringSnapshot | null>(null);
  const [draft, setDraft] = useState<api.CodexMeteringSettings | null>(null);
  const [providers, setProviders] = useState<Array<{ id: string; name: string }>>([]);
  const [filter, setFilter] = useState<api.CodexLedgerFilter>({});
  const [appliedFilter, setAppliedFilter] = useState<api.CodexLedgerFilter>({});
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<api.CodexLedgerPage | null>(null);
  const [summary, setSummary] = useState<api.CodexLedgerSummary | null>(null);
  const read = async (next: api.CodexLedgerFilter, position: number) => {
    const [ledger, totals] = await Promise.all([api.getCodexRequestLedger(next, position, 20), api.getCodexRequestSummary(next)]);
    setPage(ledger); setSummary(totals); setOffset(position); setAppliedFilter(next);
  };
  useEffect(() => { void run(async () => {
    const [settings, records] = await Promise.all([api.getCodexMetering(), listCodexProfiles()]);
    setSnapshot(settings); setDraft(settings.settings); setProviders(records.map((p) => p.profile)); await read({}, 0);
  }); }, [run]);
  if (!snapshot || !draft) return <p role="status">正在读取 Codex 本地计量…</p>;
  const dirty = JSON.stringify(snapshot.settings) !== JSON.stringify(draft);
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">此账本记录本地网关真实请求，不保存会话正文或凭据。本机会话 token 统计仍在用量工作区单独展示，不与网关重复相加。缺少价格的请求标为未计价，不视为零费用。</p>
    <PriceEditor settings={draft} onChange={setDraft} busy={busy} />
    <BillingEditor settings={draft} onChange={setDraft} providers={providers} busy={busy} />
    <div className="asb-form-actions"><Button variant="primary" disabled={busy || !dirty} onClick={() => void run(async () => { const saved = await api.setCodexMetering(draft, snapshot.revision, true); setSnapshot(saved); setDraft(saved.settings); changed("Codex 定价与预算已保存。"); })}>确认保存定价与预算</Button>
      <Button variant="secondary" disabled={busy || dirty} onClick={() => void run(async () => { const count = await api.repriceCodexRequests(appliedFilter, snapshot.revision, true); await read(appliedFilter, offset); changed(`已按当前价格回填 ${count} 条 Codex 请求。`); })}>确认按当前价格回填筛选结果</Button></div>
    <fieldset className="asb-fieldset" disabled={busy}><legend>请求筛选</legend>
      <Select ariaLabel="Codex 账本供应商" value={filter.profileId ?? "all"} options={[{ value: "all", label: "全部供应商" }, ...providers.map((p) => ({ value: p.id, label: p.name }))]} onChange={(id) => setFilter({ ...filter, profileId: id === "all" ? null : id })} />
      <label className="asb-field"><span>实际模型</span><Input value={filter.model ?? ""} onChange={(e) => setFilter({ ...filter, model: e.target.value || null })} /></label>
      <Select ariaLabel="Codex 请求结果" value={filter.outcome ?? "all"} options={[{ value: "all", label: "全部结果" }, { value: "succeeded", label: "成功" }, { value: "failed", label: "失败" }]} onChange={(v) => setFilter({ ...filter, outcome: v === "all" ? null : v as "succeeded" | "failed" })} />
      <label className="asb-field"><span>开始日期（UTC）</span><Input type="date" onChange={(e) => setFilter({ ...filter, fromMs: e.target.value ? Date.parse(e.target.value) : null })} /></label>
      <label className="asb-field"><span>结束日期（不含，UTC）</span><Input type="date" onChange={(e) => setFilter({ ...filter, untilMs: e.target.value ? Date.parse(e.target.value) : null })} /></label>
      <Button variant="secondary" onClick={() => void run(() => read(filter, 0))}>查询 Codex 请求</Button>
    </fieldset>
    {summary && <p role="status">请求 {summary.requests} · 失败 {summary.failedRequests} · 输入 {summary.inputTokens} · 输出 {summary.outputTokens} · 估算 ${summary.estimatedUsd} · 未计价 {summary.unpricedRequests}</p>}
    {page && <><Table ariaLabel="Codex 持久请求账本" rows={page.records} rowKey={(record) => record.id} columns={[
      { key: "time", header: "时间", render: (r) => new Date(r.atMs).toLocaleString() },
      { key: "model", header: "实际模型", render: (r) => r.mappedModel ?? "未记录" },
      { key: "tokens", header: "输入 / 输出 / 缓存", render: (r) => `${r.inputTokens ?? "—"} / ${r.outputTokens ?? "—"} / ${r.cacheReadTokens ?? "—"}` },
      { key: "cost", header: "费用", render: (r) => r.cost ? "$" + r.cost.totalUsd : "未计价" },
      { key: "status", header: "结果 / 尝试", render: (r) => `${r.status ?? "已取消"} / ${r.attempts.length}` },
    ]} /><div className="asb-form-actions"><Button variant="secondary" disabled={busy || offset === 0} onClick={() => void run(() => read(appliedFilter, Math.max(0, offset - 20)))}>上一页</Button>
      <span>{page.total === 0 ? 0 : offset + 1}–{offset + page.records.length} / {page.total}</span>
      <Button variant="secondary" disabled={busy || offset + page.records.length >= page.total} onClick={() => void run(() => read(appliedFilter, offset + 20))}>下一页</Button></div></>}
  </div>;
}
