import { useEffect, useState } from "react";
import * as api from "../../api/claude-prompts";
import { getGlobalPromptDocument } from "../../api/settings";
import { Button } from "../Button";
import { CodePreview } from "../CodePreview";
import { Input } from "../Input";
import { Select } from "../Select";
import { Textarea } from "../Textarea";
import { PromptSource } from "./PromptSource";
import type { ClaudeOperations } from "./operations";
const blank = (): api.ClaudePromptDraft => ({ name: "", description: null, content: "" });
function Activation({ preview, busy, onApply, onCancel }: { preview: api.ClaudePromptPreview; busy: boolean; onApply: () => void; onCancel: () => void }) {
  return <section aria-label="Claude Prompt 应用预览">
    <CodePreview target="当前 CLAUDE.md" content={preview.before} /><CodePreview target="应用后的 CLAUDE.md" content={preview.after} />
    <p className="asb-scope-note">确认后会备份并写入用户级 CLAUDE.md。库或文档在预览后发生变化时，会拒绝过期预览。</p>
    <div className="asb-form-actions"><Button variant="secondary" disabled={busy} onClick={onCancel}>取消 Prompt 应用</Button>
      <Button variant="primary" disabled={busy} onClick={onApply}>确认写入 CLAUDE.md</Button></div>
  </section>;
}
export function PromptsPane({ operations: op }: { operations: ClaudeOperations }) {
  const [view, setView] = useState<api.ClaudePromptsView | null>(null);
  const [id, setId] = useState<string | null>(null);
  const [draft, setDraft] = useState<api.ClaudePromptDraft>(blank);
  const [preview, setPreview] = useState<api.ClaudePromptPreview | null>(null);
  const [deleting, setDeleting] = useState(false);
  const { run, busy, changed } = op;
  useEffect(() => { void run(async () => setView(await api.listClaudePrompts())); }, [run]);
  const saved = view?.prompts.find((p) => p.id === id)?.draft ?? blank();
  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  const locked = busy || !!preview || !!view?.recoveryRequired;
  const select = (value: string) => { const item = view?.prompts.find((p) => p.id === value); setId(item?.id ?? null); setDraft(item?.draft ?? blank()); setDeleting(false); };
  const reorder = (delta: number) => void run(async () => {
    if (!view || !id) return; const ids = view.prompts.map((p) => p.id); const index = ids.indexOf(id); const other = index + delta;
    if (other < 0 || other >= ids.length) return; [ids[index], ids[other]] = [ids[other], ids[index]];
    setView(await api.reorderClaudePrompts(ids, view.fileHash, true));
  });
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">预设库和用户级 CLAUDE.md 分开保存。编辑使用中的预设只产生待应用内容，必须预览并确认才写入文档；不改项目指令或 Codex AGENTS.md。</p>
    {view?.recoveryRequired && <Button variant="primary" disabled={busy} onClick={() => void run(async () => setView(await api.recoverClaudePrompt(true)))}>确认恢复 Claude Prompt 未完成事务</Button>}
    {view?.externalChange && <p role="status" className="asb-scope-note">CLAUDE.md 存在外部改动；将保留现场，应用前请核对新预览。</p>}
    {view?.pendingContent && <p role="status" className="asb-scope-note">活动预设有尚未应用的修改。</p>}
    <Select ariaLabel="Claude Prompt 预设" value={id ?? "new"} disabled={locked || dirty} onChange={select}
      options={[{ value: "new", label: "新建 Prompt" }, ...(view?.prompts ?? []).map((p) => ({ value: p.id, label: p.draft.name + (p.id === view?.activePromptId ? " · 使用中" : "") }))]} />
    <label className="asb-field"><span>Claude Prompt 名称</span><Input value={draft.name} disabled={locked} onChange={(e) => setDraft({ ...draft, name: e.target.value })} /></label>
    <label className="asb-field"><span>Prompt 说明</span><Input value={draft.description ?? ""} disabled={locked} onChange={(e) => setDraft({ ...draft, description: e.target.value || null })} /></label>
    <label className="asb-field"><span>Claude Prompt 内容</span><Textarea code value={draft.content} disabled={locked} onChange={(e) => setDraft({ ...draft, content: e.target.value })} /></label>
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={locked} onClick={() => void run(async () => { const document = await getGlobalPromptDocument("claude"); setDraft({ ...draft, content: document.content }); })}>将当前 CLAUDE.md 读入草稿</Button>
      {dirty && <Button variant="secondary" disabled={locked} onClick={() => setDraft(saved)}>放弃 Prompt 草稿</Button>}
      <Button variant="primary" disabled={locked || !view || !draft.name.trim() || !dirty} onClick={() => void run(async () => {
        const next = await api.saveClaudePrompt(id, draft, view!.fileHash, true); const savedId = id ?? next.prompts.find((p) => !view!.prompts.some((old) => old.id === p.id))?.id ?? null;
        setView(next); setId(savedId); changed("Claude Prompt 已保存到库，CLAUDE.md 未改动。");
      })}>保存 Claude Prompt 到库</Button>
      <Button variant="secondary" disabled={locked || dirty || !id} onClick={() => void run(async () => setPreview(await api.previewClaudePrompt(id, view!.fileHash)))}>预览启用 Claude Prompt</Button>
      <Button variant="secondary" disabled={locked || dirty || !view?.activePromptId} onClick={() => void run(async () => setPreview(await api.previewClaudePrompt(null, view!.fileHash)))}>预览停用活动 Prompt</Button>
      <Button variant="secondary" disabled={locked || dirty || !id || view?.prompts[0]?.id === id} onClick={() => reorder(-1)}>上移 Prompt</Button>
      <Button variant="secondary" disabled={locked || dirty || !id || view?.prompts.at(-1)?.id === id} onClick={() => reorder(1)}>下移 Prompt</Button>
      <Button variant="danger" disabled={locked || dirty || !id || view?.activePromptId === id} onClick={() => setDeleting(true)}>删除 Claude Prompt</Button>
    </div>
    {deleting && id && view && <section aria-label="确认删除 Claude Prompt"><Button variant="secondary" disabled={busy} onClick={() => setDeleting(false)}>取消删除 Prompt</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => { setView(await api.removeClaudePrompt(id, view.fileHash, true)); setId(null); setDraft(blank()); setDeleting(false); })}>确认删除 Claude Prompt</Button></section>}
    {preview && <Activation preview={preview} busy={busy} onCancel={() => setPreview(null)} onApply={() => void run(async () => { setView(await api.activateClaudePrompt(preview.plan, true)); setPreview(null); changed("CLAUDE.md 已应用并备份。"); })} />}
    <Button variant="secondary" disabled={busy || dirty || !!preview} onClick={() => void run(async () => { const next = await api.listClaudePrompts(); setView(next); setId(null); setDraft(blank()); })}>重新读取 Claude Prompt 库</Button>
    {view && <PromptSource fileHash={view.fileHash} disabled={locked} operations={op} onImported={setView} />}
  </div>;
}
