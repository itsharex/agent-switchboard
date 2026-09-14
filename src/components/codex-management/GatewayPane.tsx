import { useEffect, useRef, useState } from "react";
import * as api from "../../api/codex-gateway";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Select } from "../Select";
import { PreviewInspector } from "../PreviewInspector";
import type { CodexOperations } from "./operations";
const TRAFFIC_FIELDS: Array<[keyof api.CodexTrafficSettings, string]> = [
  ["headersTimeoutSeconds", "响应头超时（秒）"], ["firstByteTimeoutSeconds", "首包超时（秒）"],
  ["idleTimeoutSeconds", "流空闲超时（秒）"], ["totalTimeoutSeconds", "总超时（秒）"],
  ["failureThreshold", "连续失败阈值"], ["cooldownSeconds", "熔断冷却（秒）"],
  ["successThreshold", "半开成功阈值"], ["errorRatePercent", "错误率阈值（%）"], ["minRequests", "错误率最少请求数"],
];
export function GatewayPane({ operations: { run, busy, changed } }: { operations: CodexOperations }) {
  const [view, setView] = useState<api.CodexPolicyView | null>(null);
  const [policy, setPolicy] = useState<api.CodexGatewayPolicy | null>(null);
  const [primary, setPrimary] = useState<string | null>(null);
  const consumed = useRef(new Set<string>());
  const [prepared, setPrepared] = useState<api.CodexPolicyPreparation | null>(null);
  useEffect(() => { void run(async () => { const next = await api.getCodexGatewayPolicy(); setView(next); setPolicy(next.policy); setPrimary(next.activeProfileId ?? next.providers[0]?.id ?? null); }); }, [run]);
  useEffect(() => () => { if (prepared && !consumed.current.has(prepared.preparationId)) void api.cancelCodexGatewayPolicy(prepared.preparationId).catch(() => {}); }, [prepared]);
  if (!view || !policy) return <p role="status">正在读取独立 Codex 网关策略…</p>;
  const disabled = busy || !!prepared || view.pendingRecovery;
  const move = (index: number, delta: number) => {
    const ids = [...policy.providerIds]; [ids[index], ids[index + delta]] = [ids[index + delta], ids[index]]; setPolicy({ ...policy, providerIds: ids });
  };
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">Responses 可直连；Chat/Anthropic 转换需要本地网关。Failover 只在显式启用后按队列工作，不包含官方账号；已经发送的流不重放。超时 0 表示不限制。</p>
    {view.warning && <p className="asb-warn-text">{view.warning}</p>}
    {view.pendingRecovery && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { const next = await api.recoverCodexGatewayPolicy(); setView(next); setPolicy(next.policy); })}>恢复 Codex 网关策略事务</Button>}
    <Select ariaLabel="Codex 主供应商" value={primary} options={view.providers.map((p) => ({ value: p.id, label: p.name }))} disabled={disabled} onChange={setPrimary} />
    <Checkbox label="接管 Codex 请求" checked={policy.takeover} disabled={disabled} onChange={(takeover) => setPolicy({ ...policy, takeover })} />
    <Checkbox label="启用 Codex Failover" checked={policy.enabled} disabled={disabled} onChange={(enabled) => setPolicy({ ...policy, enabled })} />
    <fieldset className="asb-fieldset" disabled={disabled}><legend>候选供应商</legend>{view.providers.map((provider) => <Checkbox key={provider.id} label={provider.name} checked={policy.providerIds.includes(provider.id)}
      onChange={(checked) => setPolicy({ ...policy, providerIds: checked ? [...policy.providerIds, provider.id] : policy.providerIds.filter((id) => id !== provider.id) })} />)}</fieldset>
    <ol aria-label="Codex Failover 优先级">{policy.providerIds.map((id, index) => <li key={id}>{view.providers.find((p) => p.id === id)?.name ?? "已删除的供应商"}
      <Button variant="secondary" disabled={disabled || index === 0} onClick={() => move(index, -1)} aria-label={`上移第 ${index + 1} 项`}>上移</Button>
      <Button variant="secondary" disabled={disabled || index === policy.providerIds.length - 1} onClick={() => move(index, 1)} aria-label={`下移第 ${index + 1} 项`}>下移</Button></li>)}</ol>
    <label className="asb-field"><span>最大重试次数</span><Input type="number" min={0} max={10} value={policy.maxRetries} disabled={disabled} onChange={(e) => setPolicy({ ...policy, maxRetries: Number(e.target.value) })} /></label>
    {TRAFFIC_FIELDS.map(([field, label]) => <label className="asb-field" key={field}><span>{label}</span><Input type="number" min={0} value={policy.traffic[field]} disabled={disabled}
      onChange={(e) => setPolicy({ ...policy, traffic: { ...policy.traffic, [field]: Number(e.target.value) } })} /></label>)}
    <Button variant="primary" disabled={disabled || !primary || (policy.enabled && (!policy.takeover || !policy.providerIds.length))} onClick={() => void run(async () => {
      const id = policy.enabled ? policy.providerIds[0] : primary; if (id) setPrepared(await api.prepareCodexGatewayPolicy(id, policy));
    })}>预览 Codex 策略变更</Button>
    {prepared && <section aria-label="Codex 网关策略确认"><PreviewInspector filePreview={prepared.preview} userConfigModel={null} userConfigWarnings={[]} />
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { consumed.current.add(prepared.preparationId); await api.cancelCodexGatewayPolicy(prepared.preparationId); setPrepared(null); })}>取消策略变更</Button>
      <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
        const pending = prepared; consumed.current.add(pending.preparationId); setPrepared(null); await api.commitCodexGatewayPolicy(pending.preparationId, true);
        const next = await api.getCodexGatewayPolicy(); setView(next); setPolicy(next.policy); changed("Codex 接管和故障转移策略已通过事务应用。");
      })}>确认应用 Codex 策略</Button></section>}
    <section aria-label="Codex 供应商健康状态">{view.providers.map((p) => <div key={p.id}><h4 className="asb-group-title">{p.name}</h4>
      {p.health.map((h) => <p key={h.endpoint}>{h.endpoint} · {h.circuitState} · 连续失败 {h.consecutiveFailures}</p>)}
      <Button variant="secondary" disabled={busy || !!prepared} onClick={() => void run(async () => setView(await api.resetCodexProviderHealth(p.id, true)))}>重置 {p.name} 熔断</Button></div>)}</section>
  </div>;
}
