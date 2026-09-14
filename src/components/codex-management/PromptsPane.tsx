import { useEffect, useState } from "react";
import * as api from "../../api/codex-prompts";
import { Button } from "../Button";
import { Input } from "../Input";
import { Textarea } from "../Textarea";
import { Select } from "../Select";
import { CodePreview } from "../CodePreview";
import type { CodexOperations } from "./operations";
const blank = (): api.CodexPromptDraft => ({ name: "", description: null, content: "" });
export function PromptsPane({ operations: { run, busy, changed } }: { operations: CodexOperations }) {
  const [view, setView] = useState<api.CodexPromptsView | null>(null);
  const [id, setId] = useState<string | null>(null);
  const [draft, setDraft] = useState<api.CodexPromptDraft>(blank);
  const [preview, setPreview] = useState<api.CodexPromptPreview | null>(null);
  const [deleting, setDeleting] = useState(false);
  useEffect(() => { void run(async () => setView(await api.listCodexPrompts())); }, [run]);
  if (!view) return <p role="status">正在读取 Codex 指令预设…</p>;
  const active = !!id && id === view.activeId;
  const select = (value: string) => {
    const preset = view.presets.find((p) => p.id === value); setId(preset?.id ?? null);
    setDraft(preset?.draft ?? blank()); setPreview(null); setDeleting(false);
  };
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">预设只管理用户级 AGENTS.md，不写项目文件或 CLAUDE.md。切换前会回填当前文件，未启用预设的编辑不会改动文件。</p>
    {view.pendingRecovery && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setView(await api.recoverCodexPrompt(true)))}>确认恢复未完成的指令事务</Button>}
    <Select ariaLabel="Codex 指令预设" value={id ?? "new"} disabled={busy} onChange={select} options={[{ value: "new", label: "新建指令预设" }, ...view.presets.map((p) => ({ value: p.id, label: p.draft.name + (p.id === view.activeId ? " · 使用中" : "") }))]} />
    <label className="asb-field"><span>预设名称</span><Input value={draft.name} disabled={busy} onChange={(e) => setDraft({ ...draft, name: e.target.value })} /></label>
    <label className="asb-field"><span>说明</span><Input value={draft.description ?? ""} disabled={busy} onChange={(e) => setDraft({ ...draft, description: e.target.value || null })} /></label>
    <label className="asb-field"><span>指令内容</span><Textarea code value={draft.content} disabled={busy} onChange={(e) => { setDraft({ ...draft, content: e.target.value }); setPreview(null); }} /></label>
    {active && draft.content !== view.live.content && <p className="asb-warn-text">当前 AGENTS.md 与预设内容不同；可读入草稿，切换预设时也会回填原内容。</p>}
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy} onClick={() => { setDraft({ ...draft, content: view.live.content }); setPreview(null); }}>将当前 AGENTS.md 读入草稿</Button>
      <Button variant="primary" disabled={busy || !draft.name.trim() || view.pendingRecovery} onClick={() => void run(async () => {
        setView(await api.saveCodexPrompt(id, draft, view.revision, view.live.contentHash, active));
        changed(active ? "使用中的指令预设及 AGENTS.md 已同步保存。" : "指令预设已保存，AGENTS.md 未改动。"); setPreview(null);
      })}>{active ? "确认保存并应用指令" : "保存指令预设"}</Button>
      <Button variant="secondary" disabled={busy || !id || view.pendingRecovery} onClick={() => void run(async () => setPreview(await api.previewCodexPrompt(id, view.revision)))}>预览启用预设</Button>
      <Button variant="secondary" disabled={busy || !view.activeId || view.pendingRecovery} onClick={() => void run(async () => setPreview(await api.previewCodexPrompt(null, view.revision)))}>预览停用当前指令</Button>
      <Button variant="danger" disabled={busy || !id || active || view.pendingRecovery} onClick={() => setDeleting(true)}>删除预设</Button>
    </div>
    {deleting && id && <div role="group" aria-label="确认删除指令预设"><Button variant="secondary" disabled={busy} onClick={() => setDeleting(false)}>取消删除</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => { setView(await api.deleteCodexPrompt(id, view.revision, true)); setId(null); setDraft(blank()); setDeleting(false); })}>确认删除预设</Button></div>}
    {preview && <section aria-label="Codex 指令变更预览">
      <CodePreview target="当前 AGENTS.md" content={preview.before} /><CodePreview target="应用后的 AGENTS.md" content={preview.after} />
      <p className="asb-scope-note">将备份当前全局指令；文件在预览后变化时会拒绝写入。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>取消指令应用</Button>
      <Button variant="primary" disabled={busy} onClick={() => void run(async () => { setView(await api.applyCodexPrompt(preview.plan, true)); setPreview(null); changed("Codex 全局指令已应用并备份。"); })}>确认写入 AGENTS.md</Button>
    </section>}
    <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setView(await api.listCodexPrompts()); setPreview(null); })}>重新读取指令库</Button>
  </div>;
}
