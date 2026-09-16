import { useEffect, useState } from "react";
import { listCodexPresets, prepareCodexPreset, duplicateCodexProfile, searchCodexProfiles, type CodexPresetSummary } from "../../api/codex-management";
import { createCodexProfile, type CodexProviderRecord, type CodexUpstream } from "../../api/providers";
import { createCodexUniversalProvider, deleteCodexUniversalProvider, listCodexUniversalProviders, syncCodexUniversalProfile, type CodexUniversalList } from "../../api/codex-universal";
import { ensureCodexOfficialRecord } from "../../api/official-login";
import { cancelXaiLogin, listXaiAccounts, pollXaiLogin, setXaiAccountBinding, startXaiLogin, type XaiAccountsView } from "../../api/xai-accounts";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table } from "../Table";
import { ProviderAuthentication } from "./ProviderAuthentication";
import { CodexEndpointsPanel } from "./CodexEndpointsPanel";
import type { CodexOperations } from "./operations";

const UPSTREAM_LABELS: Record<CodexUpstream, string> = {
  responses: "Responses 原生",
  chatCompletions: "Chat Completions",
  anthropicMessages: "Anthropic Messages",
};

export function ProvidersPane({ operations }: { operations: CodexOperations }) {
  const { run, busy, changed } = operations;
  const [authRecord, setAuthRecord] = useState<CodexProviderRecord | null>(null);
  const [endpointRecord, setEndpointRecord] = useState<CodexProviderRecord | null>(null);
  const [presets, setPresets] = useState<CodexPresetSummary[]>([]);
  const [records, setRecords] = useState<CodexProviderRecord[]>([]);
  const [presetId, setPresetId] = useState<string | null>(null);
  const [key, setKey] = useState(""); const [search, setSearch] = useState("");
  const [warnings, setWarnings] = useState<string[]>([]);
  const [universal, setUniversal] = useState<CodexUniversalList | null>(null);
  const [universalId, setUniversalId] = useState<string | null>(null);
  const [uName, setUName] = useState(""); const [uEndpoint, setUEndpoint] = useState("");
  const [uKey, setUKey] = useState(""); const [uModels, setUModels] = useState("");
  const [uUpstream, setUUpstream] = useState<CodexUpstream>("responses");
  const [xai, setXai] = useState<XaiAccountsView | null>(null);
  const [xaiLogin, setXaiLogin] = useState<{ userCode: string; verificationUri: string } | null>(null);
  useEffect(() => { void run(async () => { const [catalog, providers, shared, xaiView] = await Promise.all([listCodexPresets(), searchCodexProfiles(""), listCodexUniversalProviders(), listXaiAccounts()]); setPresets(catalog); setRecords(providers); setUniversal(shared); setXai(xaiView); }); }, [run]);
  const preset = presets.find((item) => item.id === presetId);
  const shared = universal?.providers.find((item) => item.id === universalId) ?? null;
  if (authRecord) return <ProviderAuthentication record={authRecord} operations={operations} onClose={() => setAuthRecord(null)} onSaved={async () => setRecords(await searchCodexProfiles(search))} />;
  if (endpointRecord) return <CodexEndpointsPanel record={endpointRecord} operations={operations} onClose={() => { setEndpointRecord(null); void run(async () => setRecords(await searchCodexProfiles(search))); }} />;
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">预设使用已审核的离线数据，并转换为本应用的独立 Codex 档案。创建或复制不会启用供应商。</p>
    <Select ariaLabel="Codex 内置预设" value={presetId} options={presets.map((p) => ({ value: p.id, label: p.name }))} disabled={busy} onChange={(id) => { setPresetId(id); setKey(""); setWarnings([]); }} />
    {preset && <p className="asb-scope-note">{preset.endpointCandidates.join(" · ")}<br />模型：{preset.models.join("、") || "由官方账号提供"}</p>}
    {preset?.authentication === "apiKey" && <label className="asb-field"><span>预设 API 密钥</span><Input type="password" autoComplete="off" value={key} disabled={busy} onChange={(e) => setKey(e.target.value)} /></label>}
    {preset?.authentication === "managedXai" && <div className="asb-provider-section-fields">
      <Select ariaLabel="xAI 账号" value={xai?.accounts[0]?.id ?? null} options={(xai?.accounts ?? []).map((a) => ({ value: a.id, label: `${a.label}${a.isDefault ? "（默认）" : ""}` }))} disabled={busy} onChange={() => {}} />
      {!xai?.accounts.length && <p className="asb-scope-note">尚未登录 xAI 账号。开始登录后，在验证页输入设备码并授权。</p>}
      {xaiLogin && <p className="asb-scope-note">设备码：{xaiLogin.userCode} · 授权页：{xaiLogin.verificationUri}</p>}
      <div className="asb-form-actions">
        {!xaiLogin && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
          const session = await startXaiLogin();
          setXaiLogin({ userCode: session.userCode, verificationUri: session.verificationUri });
        })}>登录 xAI 账号</Button>}
        {xaiLogin && <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
          const poll = await pollXaiLogin();
          if (poll.status === "completed") { setXaiLogin(null); setXai(await listXaiAccounts()); changed("xAI 账号登录成功，可以创建托管卡。"); }
        })}>我已完成授权，检查结果</Button>}
        {xaiLogin && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
          await cancelXaiLogin(); setXaiLogin(null); changed("xAI 登录已取消。");
        })}>取消登录</Button>}
      </div>
    </div>}
    <Button variant="primary" disabled={busy || !preset || (preset.authentication === "apiKey" && !key.trim()) || (preset.authentication === "managedXai" && !xai?.accounts.length)} onClick={() => void run(async () => {
      if (!preset) return;
      if (preset.authentication === "nativeOpenAi") { await ensureCodexOfficialRecord(); changed("Codex 官方档案已就绪；请到认证页选择登录方式。"); return; }
      const prepared = await prepareCodexPreset(preset.id, key);
      const record = await createCodexProfile(prepared.draft); setKey(""); setWarnings(prepared.warnings);
      if (preset.authentication === "managedXai" && xai) {
        setXai(await setXaiAccountBinding(record.profile.id, xai.accounts[0]?.id ?? null, xai.revision));
      }
      setRecords(await searchCodexProfiles(search)); changed("Codex 预设已创建，尚未启用；可回到原供应商编辑器调整模型与能力。");
    })}>从预设创建档案</Button>
    {warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
    <p className="asb-scope-note">通用连接一次定义，可生成并同步独立的 Codex 档案：连接、密钥与模型目录随通用连接更新，本端覆盖（名称、别名、参数）在同步时保留。</p>
    <Select ariaLabel="通用连接" value={universalId} options={(universal?.providers ?? []).map((p) => ({ value: p.id, label: `${p.name}（${p.linkedProfileName ? `已生成 ${p.linkedProfileName}` : "未生成"}）` }))} disabled={busy || !universal} onChange={(id) => { setUniversalId(id); setWarnings([]); }} />
    {shared && <p className="asb-scope-note">{shared.endpoint} · {UPSTREAM_LABELS[shared.upstream]} · 默认模型：{shared.defaultModel}{shared.hasApiKey ? "" : " · 未保存密钥"}</p>}
    <div className="asb-form-actions">
      <Button variant="primary" disabled={busy || !shared || !universal} onClick={() => void run(async () => {
        if (!shared || !universal) return;
        const outcome = await syncCodexUniversalProfile(shared.id, universal.revision);
        setUniversal(outcome.list); setWarnings(outcome.warnings);
        setRecords(await searchCodexProfiles(search));
        changed(outcome.created ? "已从通用连接生成独立 Codex 档案，尚未启用。" : outcome.unchanged ? "通用连接与 Codex 档案已一致，无需同步。" : "已把通用连接的连接与模型目录同步到 Codex 档案，本端覆盖保留。");
      })}>生成/同步 Codex 档案</Button>
      <Button variant="secondary" disabled={busy || !shared || !universal} onClick={() => void run(async () => {
        if (!shared || !universal) return;
        setUniversal(await deleteCodexUniversalProvider(shared.id, universal.revision));
        setUniversalId(null); changed("通用连接已删除；已生成的 Codex 档案保持独立，不受影响。");
      })}>删除通用连接</Button>
    </div>
    <div className="asb-provider-section-fields">
      <label className="asb-field"><span>通用连接名称</span><Input value={uName} disabled={busy} onChange={(e) => setUName(e.target.value)} /></label>
      <label className="asb-field"><span>服务地址</span><Input value={uEndpoint} placeholder="https://relay.example" disabled={busy} onChange={(e) => setUEndpoint(e.target.value)} /></label>
      <label className="asb-field"><span>API 密钥</span><Input type="password" autoComplete="off" value={uKey} disabled={busy} onChange={(e) => setUKey(e.target.value)} /></label>
      <label className="asb-field"><span>模型列表（逗号分隔，第一个为默认）</span><Input value={uModels} placeholder="gpt-5.2, gpt-5.1" disabled={busy} onChange={(e) => setUModels(e.target.value)} /></label>
      <Select ariaLabel="通用连接上游协议" value={uUpstream} options={Object.entries(UPSTREAM_LABELS).map(([value, label]) => ({ value, label }))} disabled={busy} onChange={(value) => setUUpstream(value as CodexUpstream)} />
      <Button variant="secondary" disabled={busy || !uName.trim() || !uEndpoint.trim() || !uModels.trim()} onClick={() => void run(async () => {
        const list = await createCodexUniversalProvider({ name: uName, endpoint: uEndpoint, upstream: uUpstream, apiKey: uKey, models: uModels.split(",").map((model) => model.trim()).filter(Boolean) });
        const created = list.providers[list.providers.length - 1];
        setUniversal(list); setUniversalId(created?.id ?? null);
        setUName(""); setUEndpoint(""); setUKey(""); setUModels("");
        if (!created) return;
        const outcome = await syncCodexUniversalProfile(created.id, list.revision);
        setUniversal(outcome.list); setRecords(await searchCodexProfiles(search));
        changed("通用连接已创建，并生成了独立 Codex 档案，尚未启用。");
      })}>创建通用连接并生成档案</Button>
    </div>
    <form onSubmit={(e) => { e.preventDefault(); void run(async () => setRecords(await searchCodexProfiles(search))); }} className="asb-form-actions">
      <Input aria-label="搜索 Codex 供应商" value={search} onChange={(e) => setSearch(e.target.value)} disabled={busy} />
      <Button type="submit" variant="secondary" disabled={busy}>搜索供应商</Button>
    </form>
    <Table ariaLabel="Codex 供应商管理" rows={records} rowKey={(r) => r.profile.id} columns={[
      { key: "name", header: "名称", render: (r) => r.profile.name },
      { key: "model", header: "默认模型", render: (r) => r.profile.defaultModel },
      { key: "authentication", header: "认证", render: (r) => <Button variant="secondary" disabled={busy} onClick={() => setAuthRecord(r)}>认证方式 {r.profile.name}</Button> },
      { key: "endpoints", header: "端点", render: (r) => <Button variant="secondary" disabled={busy} onClick={() => setEndpointRecord(r)}>服务端点 {r.profile.name}</Button> },
      { key: "copy", header: "操作", render: (r) => <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
        await duplicateCodexProfile(r.profile.id, r.fileHash); setRecords(await searchCodexProfiles(search)); changed("已创建独立副本，不会自动启用。");
      })}>复制 {r.profile.name}</Button> },
    ]} />
  </div>;
}
