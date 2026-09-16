import { useEffect, useState } from "react";
import * as api from "../../api/codex-project-plans";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import type { CodexOperations } from "./operations";

const ACTION_LABELS: Record<api.CodexProjectApplyStep["action"], string> = {
  switch: "切换供应商到", enable: "启用", disable: "停用", activate: "激活",
};
const KIND_LABELS: Record<api.CodexProjectApplyStep["kind"], string> = {
  provider: "供应商", mcp: "MCP", skill: "Skill", prompt: "指令预设",
};

function stepText(step: api.CodexProjectApplyStep): string {
  return `${KIND_LABELS[step.kind]}：${ACTION_LABELS[step.action]} ${step.label}`;
}

export function ProjectPlansPane({ operations }: { operations: CodexOperations }) {
  const { run, busy, changed } = operations;
  const [view, setView] = useState<api.CodexProjectPlansView | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [newName, setNewName] = useState("");
  const [preview, setPreview] = useState<api.CodexProjectApplyPreview | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [applied, setApplied] = useState<string[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);

  useEffect(() => { void run(async () => setView(await api.listCodexProjectPlans())); }, [run]);
  if (!view) return <p role="status">正在读取 Codex 项目方案…</p>;

  const selected = view.plans.find((plan) => plan.id === selectedId) ?? null;
  const isCurrent = !!selected && selected.id === view.current;
  const nameOf = (id: string) =>
    view.plans.find((plan) => plan.id === id)?.name
    ?? view.providers.find((option) => option.id === id)?.name
    ?? view.prompts.find((option) => option.id === id)?.name
    ?? view.bindings.find((binding) => binding.definitionId === id)?.name
    ?? id;
  const reset = (next: api.CodexProjectPlansView) => {
    setView(next); setPreview(null); setApplied([]); setWarnings([]);
  };

  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">项目方案把 Codex 的供应商、MCP、Skills 与指令预设拍成一份命名快照。应用时会先自动回拍上一个项目，再逐项通过各自的事务写入：供应商走切换执行器，MCP 与 Skills 走扩展计划管线，指令走激活事务；任一失败项单独报告，不整体回滚。Claude 的项目方案在 Claude 工作区独立管理。</p>
    <p className="asb-scope-note">当前状态：供应商 {view.activeProviderId ? nameOf(view.activeProviderId) : "无"} · 已启用 MCP {view.bindings.filter((b) => b.kind === "mcp" && b.enabled).length} 项 · 已启用 Skill {view.bindings.filter((b) => b.kind === "skill" && b.enabled).length} 项 · 指令 {view.activePromptId ? nameOf(view.activePromptId) : "无"}</p>
    <Select ariaLabel="Codex 项目方案" value={selectedId ?? "new"} disabled={busy}
      onChange={(id) => {
        const plan = view.plans.find((item) => item.id === id) ?? null;
        setSelectedId(plan?.id ?? null); setName(plan?.name ?? "");
        setPreview(null); setDeleting(false); setApplied([]); setWarnings([]);
      }}
      options={[
        { value: "new", label: "新建项目方案" },
        ...view.plans.map((plan) => ({ value: plan.id, label: plan.name + (plan.id === view.current ? " · 当前项目" : "") })),
      ]} />

    {selected && <p className="asb-scope-note">槽位：{api.describeCodexProjectSlot(selected.slot, nameOf)}</p>}

    {selected
      ? <label className="asb-field"><span>项目名称</span><Input value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
      : <label className="asb-field"><span>新项目名称</span><Input value={newName} disabled={busy} onChange={(event) => setNewName(event.target.value)} /></label>}

    <div className="asb-form-actions">
      {selected
        ? <Button variant="primary" disabled={busy || !name.trim()} onClick={() => void run(async () => {
            reset(await api.renameCodexProjectPlan(selected.id, name, view.revision));
            changed("项目方案已重命名。");
          })}>保存项目名称</Button>
        : <Button variant="primary" disabled={busy || !newName.trim()} onClick={() => void run(async () => {
            const next = await api.createCodexProjectPlan(newName, view.revision);
            reset(next);
            const created = next.plans[next.plans.length - 1] ?? null;
            setSelectedId(created?.id ?? null); setName(created?.name ?? ""); setNewName("");
            changed("已按当前状态拍下 Codex 项目方案；未改动任何配置。");
          })}>按当前状态创建项目方案</Button>}
      <Button variant="secondary" disabled={busy || !selected} onClick={() => void run(async () => {
        if (!selected) return;
        reset(await api.resnapshotCodexProjectPlan(selected.id, view.revision));
        changed("已用当前 Codex 状态重拍该项目方案的槽位。");
      })}>以当前状态重拍快照</Button>
      <Button variant="secondary" disabled={busy || !selected || isCurrent} onClick={() => void run(async () => {
        if (!selected) return;
        reset(await api.setCurrentCodexProjectPlan(selected.id, view.revision));
        changed("已把该项目方案标记为当前项目；配置未改动。");
      })}>标记为当前项目</Button>
      <Button variant="secondary" disabled={busy || !view.current} onClick={() => void run(async () => {
        reset(await api.setCurrentCodexProjectPlan(null, view.revision));
        changed("已解除当前项目标记。");
      })}>解除当前项目标记</Button>
      <Button variant="danger" disabled={busy || !selected} onClick={() => setDeleting(true)}>删除项目方案</Button>
      <Button variant="secondary" disabled={busy}
        onClick={() => void run(async () => setView(await api.listCodexProjectPlans()))}>重新读取项目方案库</Button>
    </div>

    {deleting && selected && <div role="group" aria-label="确认删除项目方案">
      <Button variant="secondary" disabled={busy} onClick={() => setDeleting(false)}>取消删除</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => {
        reset(await api.deleteCodexProjectPlan(selected.id, view.revision));
        setSelectedId(null); setName(""); setDeleting(false);
        changed("项目方案已删除；当前项目标记已同步清理，配置未改动。");
      })}>确认删除项目方案</Button>
    </div>}

    <div className="asb-form-actions">
      <Button variant="primary" disabled={busy || !selected} onClick={() => void run(async () => {
        if (!selected) return;
        setApplied([]); setWarnings([]);
        setPreview(await api.previewCodexProjectPlanApply(selected.id, view.revision));
      })}>预览应用项目方案</Button>
    </div>

    {preview && <section aria-label="Codex 项目方案应用预览">
      <p className="asb-scope-note">{preview.autosavePlanId
        ? `应用前会先把当前状态回拍到「${nameOf(preview.autosavePlanId)}」。`
        : "当前没有需要回拍的旧项目。"}</p>
      {preview.steps.length
        ? <ul>{preview.steps.map((step, index) => <li key={`${step.kind}-${step.target}-${index}`}>{stepText(step)}</li>)}</ul>
        : <p className="asb-scope-note">槽位已与当前状态一致，应用不会改动任何配置。</p>}
      {preview.warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
      <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>取消应用</Button>
        <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
          const outcome = await api.applyCodexProjectPlan(preview.planId, view.revision, true);
          setView(outcome.view); setPreview(null);
          setApplied(outcome.steps.map(stepText)); setWarnings(outcome.warnings);
          changed(outcome.warnings.length
            ? "项目方案已应用；部分条目失败，详见下方告警。"
            : "项目方案已应用，当前项目标记已更新。");
        })}>确认应用项目方案</Button>
      </div>
    </section>}

    {applied.length > 0 && <section aria-label="Codex 项目方案已应用条目">
      {applied.map((entry) => <p key={entry} className="asb-scope-note">{entry}</p>)}
    </section>}
    {warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
  </div>;
}
