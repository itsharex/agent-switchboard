import { useEffect, useRef, useState } from "react";
import * as proxy from "../../api/outbound-proxy";
import * as api from "../../api/codex-gateway";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Select } from "../Select";
import { PreviewInspector } from "../PreviewInspector";
import type { CodexOperations } from "./operations";
const TRAFFIC_FIELDS: Array<[keyof api.CodexTrafficSettings, string]> = [
  ["headersTimeoutSeconds", "响应头超时（秒）"], ["firstByteTimeoutSeconds", "流式首包超时（秒）"],
  ["idleTimeoutSeconds", "流式空闲超时（秒）"], ["totalTimeoutSeconds", "流式总超时（秒）"],
  ["nonStreamingTimeoutSeconds", "非流式总超时（秒）"],
  ["failureThreshold", "连续失败阈值"], ["cooldownSeconds", "熔断冷却（秒）"],
  ["successThreshold", "半开成功阈值"], ["errorRatePercent", "错误率阈值（%）"], ["minRequests", "错误率最少请求数"],
];
const UPSTREAM_LABELS: Record<NonNullable<api.CodexFailoverQueueMember["upstream"]>, string> = {
  responses: "Responses", chatCompletions: "Chat Completions", anthropicMessages: "Anthropic Messages",
};
export function GatewayPane({ operations: { run, busy, changed } }: { operations: CodexOperations }) {
  const [view, setView] = useState<api.CodexPolicyView | null>(null);
  const [policy, setPolicy] = useState<api.CodexGatewayPolicy | null>(null);
  const [primary, setPrimary] = useState<string | null>(null);
  const consumed = useRef(new Set<string>());
  const [prepared, setPrepared] = useState<api.CodexPolicyPreparation | null>(null);
  const [egress, setEgress] = useState<proxy.OutboundProxySettings | null>(null);
  const [proxyTest, setProxyTest] = useState<proxy.OutboundProxyTestResult | null>(null);
  const [detected, setDetected] = useState<proxy.DetectedOutboundProxy[]>([]);
  const [sourcePath, setSourcePath] = useState("");
  const [source, setSource] = useState<api.CodexFailoverSourceScan | null>(null);
  const [filled, setFilled] = useState<string | null>(null);
  useEffect(() => { void run(async () => {
    const next = await api.getCodexGatewayPolicy(); setView(next); setPolicy(next.policy); setPrimary(next.activeProfileId ?? next.providers[0]?.id ?? null);
    setEgress(await proxy.getOutboundProxy());
  }); }, [run]);
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
    <Checkbox label="上游拒绝图片时降级为文本标记重试" checked={policy.mediaFallback} disabled={disabled} onChange={(mediaFallback) => setPolicy({ ...policy, mediaFallback })} />
    <label className="asb-field"><span>最大重试次数</span><Input type="number" min={0} max={10} value={policy.maxRetries} disabled={disabled} onChange={(e) => setPolicy({ ...policy, maxRetries: Number(e.target.value) })} /></label>
    {TRAFFIC_FIELDS.map(([field, label]) => <label className="asb-field" key={field}><span>{label}</span><Input type="number" min={0} value={policy.traffic[field]} disabled={disabled}
      onChange={(e) => setPolicy({ ...policy, traffic: { ...policy.traffic, [field]: Number(e.target.value) } })} /></label>)}
    <fieldset className="asb-fieldset" disabled={disabled}><legend>从本机导入 Codex 故障转移策略</legend>
      <p className="asb-scope-note">只读扫描来源数据库中的 Codex 队列与代理策略；填入表单后仍需通过上方预览与确认应用，不会绕过事务写入。</p>
      <label className="asb-field"><span>导入源数据库路径</span><Input value={sourcePath} placeholder="输入源数据库路径" onChange={(e) => { setSourcePath(e.target.value); setSource(null); setFilled(null); }} /></label>
      <Button variant="secondary" disabled={disabled || !sourcePath.trim()} onClick={() => void run(async () => { setFilled(null); setSource(await api.scanCodexFailoverSource(sourcePath.trim())); })}>只读扫描 Codex 故障转移队列</Button>
      {source && !source.found && <p role="status">来源没有 Codex 故障转移数据。</p>}
      {source && source.found && <section aria-label="Codex 故障转移队列预览">
        <ul aria-label="来源队列成员">{source.members.map((member) => <li key={member.sourceName}>
          {member.sourceName} · {member.endpoint ? `${member.endpoint}（${member.upstream ? UPSTREAM_LABELS[member.upstream] : "未知协议"}）` : "无可用路由"} · {member.matchedProfileName ? `本地档案：${member.matchedProfileName}` : "未匹配（请先导入该供应商）"}
        </li>)}</ul>
        {source.warnings.map((warning) => <p className="asb-scope-note" key={warning}>{warning}</p>)}
        <p className="asb-scope-note">提案：接管 {source.proposal.takeover ? "开" : "关"} · Failover {source.proposal.enabled ? "开" : "关"} · 队列 {source.proposal.providerIds.length} 项 · 重试 {source.proposal.maxRetries} 次。</p>
        <Button variant="primary" disabled={disabled} onClick={() => {
          setPolicy(source.proposal);
          if (source.proposal.enabled) setPrimary(source.proposal.providerIds[0] ?? null);
          setSource(null); setFilled("已把来源策略填入表单；请预览并确认应用 Codex 策略。");
        }}>填入策略表单</Button>
      </section>}
      {filled && <p role="status">{filled}</p>}
    </fieldset>
    <Button variant="primary" disabled={disabled || !primary || (policy.enabled && (!policy.takeover || !policy.providerIds.length))} onClick={() => void run(async () => {
      const id = policy.enabled ? policy.providerIds[0] : primary; if (id) setPrepared(await api.prepareCodexGatewayPolicy(id, policy));
    })}>预览 Codex 策略变更</Button>
    {prepared && <section aria-label="Codex 网关策略确认"><PreviewInspector filePreview={prepared.preview} userConfigModel={null} userConfigWarnings={[]} />
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { consumed.current.add(prepared.preparationId); await api.cancelCodexGatewayPolicy(prepared.preparationId); setPrepared(null); })}>取消策略变更</Button>
      <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
        const pending = prepared; consumed.current.add(pending.preparationId); setPrepared(null); await api.commitCodexGatewayPolicy(pending.preparationId, true);
        const next = await api.getCodexGatewayPolicy(); setView(next); setPolicy(next.policy); changed("Codex 接管和故障转移策略已通过事务应用。");
      })}>确认应用 Codex 策略</Button></section>}
    {egress && <fieldset className="asb-fieldset" disabled={busy || !!prepared}><legend>出站代理</legend>
      <p className="asb-scope-note">本机网关、连接探测与真实请求统一走此代理；保存后立即生效，无需重启。系统代理指向本网关自身时自动直连防回环。</p>
      <Select ariaLabel="出站代理模式" value={egress.mode} options={[
        { value: "none", label: "直连（不使用代理）" },
        { value: "system", label: "跟随系统代理" },
        { value: "manual", label: "手动指定代理" },
      ]} onChange={(mode) => setEgress({ ...egress, mode: mode as proxy.OutboundProxyMode, url: mode === "manual" ? egress.url ?? "" : null })} />
      {egress.mode === "manual" && <label className="asb-field"><span>代理地址（http/https/socks5/socks5h，可含认证）</span>
        <Input value={egress.url ?? ""} placeholder="http://127.0.0.1:7890" onChange={(e) => setEgress({ ...egress, url: e.target.value || null })} /></label>}
      <div className="asb-form-actions">
        <Button variant="primary" disabled={busy || (egress.mode === "manual" && !egress.url)} onClick={() => void run(async () => {
          const saved = await proxy.setOutboundProxy(egress, egress.revision, true); setEgress(saved); setProxyTest(null); changed("出站代理设置已保存并即时生效。");
        })}>保存出站代理</Button>
        {egress.mode === "manual" && egress.url && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
          setProxyTest(await proxy.testOutboundProxy(egress.url ?? ""));
        })}>测试代理连通性</Button>}
        <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setDetected(await proxy.scanLocalOutboundProxies()))}>扫描本机代理端口</Button>
      </div>
      {proxyTest && <p role="status">{proxyTest.success ? `代理可用，延迟 ${proxyTest.latencyMs} ms。` : `代理不可用：${proxyTest.error ?? "未知错误"}`}</p>}
      {detected.length > 0 && <ul aria-label="本机检测到的代理">{detected.map((item) => <li key={item.url}>
        <Button variant="secondary" disabled={busy} onClick={() => setEgress({ ...egress, mode: "manual", url: item.url })}>{item.url}</Button>
      </li>)}</ul>}
    </fieldset>}
    <section aria-label="Codex 供应商健康状态">{view.providers.map((p) => <div key={p.id}><h4 className="asb-group-title">{p.name}</h4>
      {p.health.map((h) => <p key={h.endpoint}>{h.endpoint} · {h.circuitState} · 连续失败 {h.consecutiveFailures}</p>)}
      <Button variant="secondary" disabled={busy || !!prepared} onClick={() => void run(async () => setView(await api.resetCodexProviderHealth(p.id, true)))}>重置 {p.name} 熔断</Button></div>)}</section>
  </div>;
}
