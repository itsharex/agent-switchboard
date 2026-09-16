import { useEffect, useState } from "react";
import { extractCodexCommonConfig, getCodexCommonConfig, saveCodexCommonFragment, setCodexCommonConfigEnabled, type CodexCommonView } from "../../api/codex-common";
import { getClientSettingsEditor, saveClientSettings, type ClientSettingsEditor, type SettingsValues } from "../../api/settings";
import { listCodexProfiles, listProfiles } from "../../api/providers";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Select } from "../Select";
import { SettingsFields } from "../SettingsFields";
import { Textarea } from "../Textarea";
import { SwitchConfirmation } from "./SwitchConfirmation";
import type { CodexOperations } from "./operations";
export function CommonPane({ operations: op }: { operations: CodexOperations }) {
  const [view, setView] = useState<CodexCommonView | null>(null);
  const [editor, setEditor] = useState<ClientSettingsEditor | null>(null);
  const [draft, setDraft] = useState<SettingsValues>({ settings: {} });
  const [fragmentDraft, setFragmentDraft] = useState("");
  const [providers, setProviders] = useState<Array<{ value: string; label: string }>>([]);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const { run, busy, changed } = op;
  useEffect(() => { void run(async () => {
    const [common, fields, thirdParty, generic] = await Promise.all([getCodexCommonConfig(), getClientSettingsEditor("codex"), listCodexProfiles(), listProfiles()]);
    setView(common); setEditor(fields); setDraft(common.settings.settings); setFragmentDraft(common.fragment.text);
    const profiles = [...thirdParty.map((p) => p.profile), ...generic.filter((p) => p.profile.app === "codex").map((p) => p.profile)];
    setProviders(profiles.map((p) => ({ value: p.id, label: p.name }))); setProfileId(profiles[0]?.id ?? null);
  }); }, [run]);
  if (!view || !editor) return <p role="status">正在读取可视化通用配置…</p>;
  const dirty = JSON.stringify(draft) !== JSON.stringify(view.settings.settings);
  const fragmentDirty = fragmentDraft !== view.fragment.text;
  const enabled = !!profileId && !view.policy.disabledProfileIds.includes(profileId);
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">沿用本应用的唯一通用配置文件与可视化字段。提取只读取通用字段，不复制供应商、MCP、项目配置或凭据。保存文件与应用到客户端分开确认。</p>
    <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setDraft(await extractCodexCommonConfig()); setApplying(false); })}>从当前 Codex 配置提取（只读）</Button>
    <SettingsFields specs={editor.specs} groups={editor.groups} values={draft.settings} baselineValues={view.settings.settings.settings} busy={busy}
      onChange={(key, value) => { setDraft((old) => ({ settings: { ...old.settings, [key]: value } })); setApplying(false); }}
      onResetGroup={(group) => { setDraft((old) => ({ settings: Object.fromEntries(Object.entries(old.settings).map(([key, value]) => [key, !group || editor.specs.some((s) => s.key === key && s.group === group) ? { mode: "automatic" } : value])) })); setApplying(false); }} />
    <Button variant="primary" disabled={busy || !dirty} onClick={() => void run(async () => {
      await saveClientSettings("codex", draft, view.settings.settingsHash); setView(await getCodexCommonConfig()); changed("通用配置文件已保存；客户端尚未改变，请预览并应用。");
    })}>保存可视化通用配置</Button>
    <label className="asb-field">
      <span>高级 TOML 片段（MCP、自定义供应商等宿主键；不可包含可视化字段或路由键）</span>
      <Textarea code rows={8} value={fragmentDraft} disabled={busy} placeholder={"# 例：\n[mcp_servers.tools]\ncommand = 'node'\nargs = ['server.js']"} onChange={(e) => { setFragmentDraft(e.target.value); setApplying(false); }} />
    </label>
    <Button variant="primary" disabled={busy || !fragmentDirty} onClick={() => void run(async () => {
      setView(await saveCodexCommonFragment(fragmentDraft, view.fragment.revision)); changed("通用配置片段已保存；片段与可视化开关共用“此供应商应用通用配置”，确认应用后生效。");
    })}>保存通用配置片段</Button>
    <Select ariaLabel="通用配置目标供应商" value={profileId} options={providers} disabled={busy} onChange={(id) => { setProfileId(id); setApplying(false); }} />
    <Checkbox label="此供应商应用通用配置" checked={enabled} disabled={busy || !profileId} onChange={(next) => void run(async () => {
      if (profileId) { setView(await setCodexCommonConfigEnabled(profileId, next, view.revision)); setApplying(false); changed("应用策略已保存，确认应用后生效；停用时恢复自动值并剥离片段，不改通用文件。"); }
    })} />
    <Button variant="secondary" disabled={busy || dirty || !profileId} onClick={() => setApplying(true)}>预览并同步通用配置</Button>
    {applying && profileId && <SwitchConfirmation profileId={profileId} operations={op} onClose={() => setApplying(false)} />}
  </div>;
}
