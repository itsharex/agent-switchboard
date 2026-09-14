import { useEffect, useState } from "react";
import * as api from "../../api/claude-providers";
import { getClaudeAccounts, type ClaudeAccountsView } from "../../api/claude-accounts";
import type { ProviderRecord } from "../../api/providers";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { PresetForm } from "./PresetForm";
import { ConnectionForm } from "./ConnectionForm";
import type { ClaudeOperations } from "./operations";
export function ProvidersPane({ operations: op }: { operations: ClaudeOperations }) {
  const [presets, setPresets] = useState<api.ClaudePresetSummary[]>([]);
  const [accounts, setAccounts] = useState<ClaudeAccountsView | null>(null);
  const [records, setRecords] = useState<ProviderRecord[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [copyName, setCopyName] = useState("");
  const [mode, setMode] = useState("presets");
  const { run, busy, changed } = op;
  useEffect(() => { void run(async () => {
    const [catalog, profiles, view] = await Promise.all([api.listClaudePresets(), api.searchClaudeProfiles(""), getClaudeAccounts()]);
    setPresets(catalog); setRecords(profiles); setAccounts(view);
  }); }, [run]);
  const refresh = async () => { setRecords(await api.searchClaudeProfiles(query)); changed("Claude 档案已保存；尚未启用的档案请在供应商页预览启用。"); };
  const record = records.find((r) => r.profile.id === selected);
  const select = (id: string | null) => { setSelected(id); setCopyName((records.find((r) => r.profile.id === id)?.profile.name ?? "") + " 副本"); };
  return <div className="asb-provider-section-fields">
    <Select ariaLabel="Claude 供应商管理操作" value={mode} disabled={busy} onChange={setMode}
      options={[{ value: "presets", label: "离线供应商预设" }, { value: "profiles", label: "档案连接、绑定与复制" }]} />
    {mode === "presets" && accounts && <PresetForm presets={presets} accounts={accounts.accounts} operations={op} onSaved={refresh} />}
    {mode === "profiles" && <>
      <div className="asb-form-actions"><Input aria-label="搜索 Claude 档案" value={query} disabled={busy} onChange={(e) => setQuery(e.target.value)} />
        <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setRecords(await api.searchClaudeProfiles(query)); select(null); })}>搜索档案</Button></div>
      <Select ariaLabel="Claude 本地档案" value={selected} disabled={busy} placeholder="选择档案" onChange={select}
        options={records.map((r) => ({ value: r.profile.id, label: r.profile.name }))} />
      {record && <>
        <ConnectionForm key={record.profile.id + record.fileHash} record={record} accounts={accounts?.accounts ?? []} operations={op} onSaved={refresh} />
        {record.profile.routeMode === "custom" && <div className="asb-form-actions">
          <Input aria-label="Claude 副本名称" value={copyName} disabled={busy} onChange={(e) => setCopyName(e.target.value)} />
          <Button variant="secondary" disabled={busy || !copyName.trim()} onClick={() => void run(async () => {
            await api.duplicateClaudeProfile(record.profile.id, copyName.trim(), record.fileHash, true); await refresh();
          })}>确认创建副本</Button>
        </div>}
      </>}
    </>}
    {!accounts && <p role="status">正在读取 Claude 供应商与账号…</p>}
  </div>;
}
