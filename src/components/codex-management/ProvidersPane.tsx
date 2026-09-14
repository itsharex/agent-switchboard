import { useEffect, useState } from "react";
import { listCodexPresets, prepareCodexPreset, duplicateCodexProfile, searchCodexProfiles, type CodexPresetSummary } from "../../api/codex-management";
import { createCodexProfile, type CodexProviderRecord } from "../../api/providers";
import { ensureCodexOfficialRecord } from "../../api/official-login";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table } from "../Table";
import { ProviderAuthentication } from "./ProviderAuthentication";
import type { CodexOperations } from "./operations";
export function ProvidersPane({ operations }: { operations: CodexOperations }) {
  const { run, busy, changed } = operations;
  const [authRecord, setAuthRecord] = useState<CodexProviderRecord | null>(null);
  const [presets, setPresets] = useState<CodexPresetSummary[]>([]);
  const [records, setRecords] = useState<CodexProviderRecord[]>([]);
  const [presetId, setPresetId] = useState<string | null>(null);
  const [key, setKey] = useState(""); const [search, setSearch] = useState("");
  const [warnings, setWarnings] = useState<string[]>([]);
  useEffect(() => { void run(async () => { const [catalog, providers] = await Promise.all([listCodexPresets(), searchCodexProfiles("")]); setPresets(catalog); setRecords(providers); }); }, [run]);
  const preset = presets.find((item) => item.id === presetId);
  if (authRecord) return <ProviderAuthentication record={authRecord} operations={operations} onClose={() => setAuthRecord(null)} onSaved={async () => setRecords(await searchCodexProfiles(search))} />;
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">预设固定于已审核的 CC Switch 版本，并转换为本应用的独立 Codex 档案。创建或复制不会启用供应商。</p>
    <Select ariaLabel="Codex 内置预设" value={presetId} options={presets.map((p) => ({ value: p.id, label: p.name }))} disabled={busy} onChange={(id) => { setPresetId(id); setKey(""); setWarnings([]); }} />
    {preset && <p className="asb-scope-note">{preset.endpointCandidates.join(" · ")}<br />模型：{preset.models.join("、") || "由官方账号提供"}</p>}
    {preset?.authentication === "apiKey" && <label className="asb-field"><span>预设 API 密钥</span><Input type="password" autoComplete="off" value={key} disabled={busy} onChange={(e) => setKey(e.target.value)} /></label>}
    {preset?.authentication === "managedXai" && <p className="asb-warn-text">此预设需要 xAI 托管账号，不能用普通 API 密钥创建。</p>}
    <Button variant="primary" disabled={busy || !preset || preset.authentication === "managedXai" || (preset.authentication === "apiKey" && !key.trim())} onClick={() => void run(async () => {
      if (!preset) return;
      if (preset.authentication === "nativeOpenAi") { await ensureCodexOfficialRecord(); changed("Codex 官方档案已就绪；请到认证页选择登录方式。"); return; }
      const prepared = await prepareCodexPreset(preset.id, key);
      await createCodexProfile(prepared.draft); setKey(""); setWarnings(prepared.warnings);
      setRecords(await searchCodexProfiles(search)); changed("Codex 预设已创建，尚未启用；可回到原供应商编辑器调整模型与能力。");
    })}>从预设创建档案</Button>
    {warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
    <form onSubmit={(e) => { e.preventDefault(); void run(async () => setRecords(await searchCodexProfiles(search))); }} className="asb-form-actions">
      <Input aria-label="搜索 Codex 供应商" value={search} onChange={(e) => setSearch(e.target.value)} disabled={busy} />
      <Button type="submit" variant="secondary" disabled={busy}>搜索供应商</Button>
    </form>
    <Table ariaLabel="Codex 供应商管理" rows={records} rowKey={(r) => r.profile.id} columns={[
      { key: "name", header: "名称", render: (r) => r.profile.name },
      { key: "model", header: "默认模型", render: (r) => r.profile.defaultModel },
      { key: "authentication", header: "认证", render: (r) => <Button variant="secondary" disabled={busy} onClick={() => setAuthRecord(r)}>认证方式 {r.profile.name}</Button> },
      { key: "copy", header: "操作", render: (r) => <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
        await duplicateCodexProfile(r.profile.id, r.fileHash); setRecords(await searchCodexProfiles(search)); changed("已创建独立副本，不会自动启用。");
      })}>复制 {r.profile.name}</Button> },
    ]} />
  </div>;
}
