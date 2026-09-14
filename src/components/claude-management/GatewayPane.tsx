import { useEffect, useState } from "react";
import * as api from "../../api/claude-gateway";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { DiffView } from "../DiffView";
import { Table } from "../Table";
import type { ClaudeOperations } from "./operations";
const FIELDS: [Exclude<keyof api.ClaudeTrafficSettings, "restoreOnExit">, string, number][] = [
  ["headersTimeoutSeconds", "响应头超时（秒）", 600], ["streamingFirstByteTimeoutSeconds", "流式首包超时（秒）", 3600],
  ["streamingIdleTimeoutSeconds", "流式空闲超时（秒）", 3600], ["nonStreamingTimeoutSeconds", "非流式总超时（秒）", 7200],
  ["circuitFailureThreshold", "连续失败熔断阈值", 100], ["circuitSuccessThreshold", "熔断恢复成功阈值", 100],
  ["circuitCooldownSeconds", "熔断冷却时间（秒）", 3600], ["circuitErrorRatePercent", "熔断错误率（%）", 100], ["circuitMinRequests", "错误率最小样本数", 10000],
];
function TrafficFields({ policy, change, disabled }: { policy: api.ClaudeFailoverPolicy; change: (value: api.ClaudeFailoverPolicy) => void; disabled: boolean }) {
  return <>
    <Checkbox label="接管 Claude 原生协议请求" checked={policy.takeover} disabled={disabled} onChange={(takeover) => change({ ...policy, takeover })} />
    <Checkbox label="应用退出时恢复 Claude 接管前配置" checked={policy.traffic.restoreOnExit} disabled={disabled} onChange={(restoreOnExit) => change({ ...policy, traffic: { ...policy.traffic, restoreOnExit } })} />
    <Checkbox label="启用 Claude Failover" checked={policy.enabled} disabled={disabled} onChange={(enabled) => change({ ...policy, enabled })} />
    <label className="asb-field"><span>最多重试次数（首次请求之外）</span><Input type="number" min={0} max={10} value={policy.maxRetries} disabled={disabled} onChange={(e) => change({ ...policy, maxRetries: Number(e.target.value) })} /></label>
    <div className="asb-provider-field-grid">{FIELDS.map(([key, label, max]) => <label className="asb-field" key={key}><span>{label}</span>
      <Input type="number" min={1} max={max} value={policy.traffic[key]} disabled={disabled} onChange={(e) => change({ ...policy, traffic: { ...policy.traffic, [key]: Number(e.target.value) } })} /></label>)}</div>
  </>;
}
export function GatewayPane({ operations: op }: { operations: ClaudeOperations }) {
  const [view, setView] = useState<api.ClaudeFailoverView | null>(null);
  const [policy, setPolicy] = useState<api.ClaudeFailoverPolicy | null>(null);
  const [stop, setStop] = useState<api.ClaudeGatewayStopPreview | null>(null);
  const { run, busy, changed } = op;
  const accept = (next: api.ClaudeFailoverView) => { setView(next); setPolicy(next.policy); };
  useEffect(() => { void run(async () => accept(await api.getClaudeFailover())); }, [run]);
  const move = (id: string, delta: number) => {
    if (!policy) return; const ids = [...policy.providerIds]; const index = ids.indexOf(id); const other = index + delta;
    if (index < 0 || other < 0 || other >= ids.length) return; [ids[index], ids[other]] = [ids[other], ids[index]]; setPolicy({ ...policy, providerIds: ids });
  };
  const ordered = view && policy ? [...view.providers].sort((a, b) => {
    const index = (id: string) => policy.providerIds.includes(id) ? policy.providerIds.indexOf(id) : Number.MAX_SAFE_INTEGER;
    return index(a.id) - index(b.id);
  }) : [];
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">仅影响 Claude。未接管时保存策略不会擅自写入客户端配置，请在供应商页重新预览启用。开始向客户端发送内容后，不会再偷偷换供应商。</p>
    {policy && <TrafficFields policy={policy} change={setPolicy} disabled={busy || !!stop} />}
    {view?.warnings.map((warning, i) => <p className="asb-scope-note" key={i}>{warning}</p>)}
    {policy && <Table ariaLabel="Claude Failover 队列" rows={ordered} rowKey={(p) => p.id} columns={[
      { key: "include", header: "队列", render: (p) => <Checkbox label={'将 ' + p.name + ' 纳入 Claude 队列'} checked={policy.providerIds.includes(p.id)} disabled={busy || !p.routeable || !!stop}
        onChange={(enabled) => setPolicy({ ...policy, providerIds: enabled ? [...policy.providerIds, p.id] : policy.providerIds.filter((id) => id !== p.id) })} /> },
      { key: "name", header: "供应商", render: (p) => p.name },
      { key: "health", header: "健康", render: (p) => !p.routeable ? "原生云 SDK（不参与队列）" : (p.health.state === "closed" ? "正常" : p.health.state === "open" ? "熔断" : "探测恢复") + ' · ' + p.health.failedRequests + '/' + p.health.totalRequests },
      { key: "order", header: "操作", render: (p) => <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy || policy.providerIds.indexOf(p.id) <= 0 || !!stop} onClick={() => move(p.id, -1)}>上移</Button>
        <Button variant="secondary" disabled={busy || !policy.providerIds.includes(p.id) || policy.providerIds.indexOf(p.id) === policy.providerIds.length - 1 || !!stop} onClick={() => move(p.id, 1)}>下移</Button>
        <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setView(await api.resetClaudeProviderHealth(p.id, true)); changed("Claude 供应商健康状态已重置，未保存的策略草稿保持不变。"); })}>重置健康</Button>
      </div> },
    ]} />}
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy || !!stop} onClick={() => void run(async () => accept(await api.getClaudeFailover()))}>重新读取 Claude 策略</Button>
      <Button variant="primary" disabled={busy || !policy || !!stop} onClick={() => void run(async () => {
        if (!policy || !Number.isInteger(policy.maxRetries) || policy.maxRetries < 0 || policy.maxRetries > 10) throw new Error("重试次数必须在 0–10 之间");
        for (const [key, label, max] of FIELDS) if (!Number.isInteger(policy.traffic[key]) || policy.traffic[key] < 1 || policy.traffic[key] > max) throw new Error(label + '必须在 1–' + max + ' 之间');
        const next = await api.setClaudeFailoverPolicy(policy, true); accept(next); changed(next.warnings.join("；") || "Claude 网关策略已保存。");
      })}>确认保存 Claude 请求策略</Button>
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setStop(await api.previewClaudeGatewayStop()))}>预览停止 Claude 接管</Button>
    </div>
    {stop && <section aria-label="停止 Claude 接管预览"><p>{stop.target}</p><DiffView changes={stop.changes} label="Claude 恢复变更" />
      <Button variant="secondary" disabled={busy} onClick={() => setStop(null)}>取消停止接管</Button>
      <Button variant="primary" disabled={busy} onClick={() => void run(async () => { const result = await api.stopClaudeGateway(stop, true); setStop(null); accept(await api.getClaudeFailover()); changed("Claude 接管已停止并恢复配置。" + result.warnings.join("；")); })}>确认停止并恢复 Claude 配置</Button>
    </section>}
  </div>;
}
