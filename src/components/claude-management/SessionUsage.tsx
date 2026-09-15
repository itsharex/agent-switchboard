import { useState } from "react";
import * as api from "../../api/claude-ledger";
import { Button } from "../Button";
import { Table } from "../Table";
import type { ClaudeOperations } from "./operations";
/** Claude CLI session usage from the local JSONL transcripts. Reads are
 * incremental; a rebuild backs the ledger up first. */
export function SessionUsage({ operations: op }: { operations: ClaudeOperations }) {
  const [view, setView] = useState<api.ClaudeSessionUsageView | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const { run, busy, changed } = op;
  const summary = view?.summary;
  return <section aria-label="Claude 会话用量" className="asb-provider-section-fields">
    <h3 className="asb-section-title">CLI 会话用量</h3>
    <p className="asb-scope-note">来自本机 Claude Code 会话记录，与网关请求账本分开计数；「已匹配网关」表示同一请求已在请求账本中，不应重复相加。参考价按本地价格表估算，非账单。</p>
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setView(await api.getClaudeSessionUsage()))}>同步并读取 Claude 会话用量</Button>
      <Button variant="secondary" disabled={busy || !view || rebuilding} onClick={() => setRebuilding(true)}>重建会话用量账本</Button>
    </div>
    {rebuilding && <section aria-label="确认重建 Claude 会话用量账本"><p>先备份当前账本，再清空条目与游标并重新扫描全部会话记录。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setRebuilding(false)}>取消重建</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => {
        const outcome = await api.rebuildClaudeSessionUsage(true); setView(outcome); setRebuilding(false);
        changed(`Claude 会话用量已重建，导入 ${outcome.report.imported} 条${outcome.backupFile ? `，备份 ${outcome.backupFile}` : ""}。`);
      })}>确认重建 Claude 会话用量</Button></section>}
    {view && <>
      <p role="status">扫描 {view.report.filesScanned} 个文件 · 新增 {view.report.imported} · 更新 {view.report.updated} · 匹配网关 {view.report.gatewayMatched}{view.report.pinnedRewrites > 0 ? ` · 检测到 ${view.report.pinnedRewrites} 处外部改写` : ""}</p>
      {view.report.errors.map((error, index) => <p className="asb-field-error" key={index}>{error}</p>)}
    </>}
    {summary && <>
      <p>请求 {summary.requests} · 已匹配网关 {summary.gatewayMatched} · 已计价 {summary.pricedRequests} · 参考费用 USD {summary.estimatedCostUsd}</p>
      <p>输入 {summary.inputTokens} · 输出 {summary.outputTokens} · 缓存读取 {summary.cacheReadTokens} · 缓存写入 {summary.cacheCreationTokens}</p>
      <Table ariaLabel="Claude 会话用量按模型" rows={summary.byModel} rowKey={(row) => row.model} columns={[
        { key: "model", header: "模型", render: (row) => row.model },
        { key: "requests", header: "请求", render: (row) => row.requests },
        { key: "tokens", header: "输入 / 输出", render: (row) => row.inputTokens + " / " + row.outputTokens },
        { key: "cost", header: "参考费用 USD", render: (row) => row.pricedRequests === 0 ? "未计价" : row.estimatedCostUsd + (row.pricedRequests < row.requests ? "（部分）" : "") },
      ]} />
    </>}
  </section>;
}
