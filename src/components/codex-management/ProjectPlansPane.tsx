import { useEffect, useState } from "react";
import * as api from "../../api/codex-project-plans";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { ClientManagementModule } from "../client-management/ClientManagementModule";
import type { CodexOperations } from "./operations";

const ACTION_LABELS: Record<api.CodexProjectApplyStep["action"], string> = {
  switch: "切换至", enable: "启用", disable: "停用", activate: "激活",
};
const KIND_LABELS: Record<api.CodexProjectApplyStep["kind"], string> = {
  provider: "供应商", mcp: "MCP", skill: "Skill", prompt: "指令预设",
};

function stepText(step: api.CodexProjectApplyStep): string {
  return `${KIND_LABELS[step.kind]}：${ACTION_LABELS[step.action]} ${step.label}`;
}

function projectPlanName(view: api.CodexProjectPlansView, id: string): string {
  return view.plans.find((plan) => plan.id === id)?.name
    ?? view.providers.find((option) => option.id === id)?.name
    ?? view.prompts.find((option) => option.id === id)?.name
    ?? view.bindings.find((binding) => binding.definitionId === id)?.name
    ?? id;
}

interface ProjectPlanDetailsProps {
  view: api.CodexProjectPlansView;
  operations: CodexOperations;
  selectedId: string | null;
  name: string;
  newName: string;
  setSelectedId: (id: string | null) => void;
  setName: (name: string) => void;
  setNewName: (name: string) => void;
  setPreview: (preview: api.CodexProjectApplyPreview | null) => void;
  setDeleting: (deleting: boolean) => void;
  clearOutcome: () => void;
  reset: (next: api.CodexProjectPlansView) => void;
}

function ProjectPlanDetails({
  view,
  operations,
  selectedId,
  name,
  newName,
  setSelectedId,
  setName,
  setNewName,
  setPreview,
  setDeleting,
  clearOutcome,
  reset,
}: ProjectPlanDetailsProps) {
  const { run, busy, changed } = operations;
  const selected = view.plans.find((plan) => plan.id === selectedId) ?? null;
  const nameOf = (id: string) => projectPlanName(view, id);

  return <section className="asb-client-management-group asb-project-plan-details" aria-label="工作场景内容">
    <h4 className="asb-group-title">场景内容</h4>
    <p className="asb-scope-note">当前状态 · 供应商：{view.activeProviderId ? nameOf(view.activeProviderId) : "无"} · MCP：{view.bindings.filter((binding) => binding.kind === "mcp" && binding.enabled).length} · Skills：{view.bindings.filter((binding) => binding.kind === "skill" && binding.enabled).length} · 指令：{view.activePromptId ? nameOf(view.activePromptId) : "无"}</p>
    <Select ariaLabel="选择工作场景" value={selectedId ?? "new"} disabled={busy}
      onChange={(id) => {
        const plan = view.plans.find((item) => item.id === id) ?? null;
        setSelectedId(plan?.id ?? null); setName(plan?.name ?? "");
        setPreview(null); setDeleting(false); clearOutcome();
      }}
      options={[
        { value: "new", label: "创建新场景" },
        ...view.plans.map((plan) => ({ value: plan.id, label: plan.name + (plan.id === view.current ? " · 当前场景" : "") })),
      ]} />
    {selected && <p className="asb-scope-note">场景包含：{api.describeCodexProjectSlot(selected.slot, nameOf)}</p>}
    {selected
      ? <label className="asb-field"><span>场景名称</span><Input value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
      : <label className="asb-field"><span>新场景名称</span><Input value={newName} disabled={busy} onChange={(event) => setNewName(event.target.value)} /></label>}
    <div className="asb-form-actions">
      {selected
        ? <Button variant="primary" disabled={busy || !name.trim()} onClick={() => void run(async () => {
            reset(await api.renameCodexProjectPlan(selected.id, name, view.revision));
            changed("场景名称已保存。");
          })}>保存名称</Button>
        : <Button variant="primary" disabled={busy || !newName.trim()} onClick={() => void run(async () => {
            const next = await api.createCodexProjectPlan(newName, view.revision);
            reset(next);
            const created = next.plans[next.plans.length - 1] ?? null;
            setSelectedId(created?.id ?? null); setName(created?.name ?? ""); setNewName("");
            changed("当前工作状态已保存为场景；尚未修改 Codex 配置。");
          })}>保存当前场景</Button>}
    </div>
  </section>;
}
interface ProjectPlanActionsProps {
  view: api.CodexProjectPlansView;
  operations: CodexOperations;
  selectedId: string | null;
  deleting: boolean;
  setSelectedId: (id: string | null) => void;
  setName: (name: string) => void;
  setPreview: (preview: api.CodexProjectApplyPreview | null) => void;
  setDeleting: (deleting: boolean) => void;
  clearOutcome: () => void;
  reset: (next: api.CodexProjectPlansView) => void;
}

function ProjectPlanActions({
  view,
  operations,
  selectedId,
  deleting,
  setSelectedId,
  setName,
  setPreview,
  setDeleting,
  clearOutcome,
  reset,
}: ProjectPlanActionsProps) {
  const { run, busy, changed } = operations;
  const selected = view.plans.find((plan) => plan.id === selectedId) ?? null;
  const isCurrent = !!selected && selected.id === view.current;

  const setCurrent = () => void run(async () => {
    if (!selected) return;
    reset(await api.setCurrentCodexProjectPlan(isCurrent ? null : selected.id, view.revision));
    changed(isCurrent ? "已取消当前场景标记。" : "已设为当前场景；尚未修改 Codex 配置。");
  });

  return <>
    <section className="asb-client-management-group asb-project-plan-actions" aria-label="场景操作">
      <h4 className="asb-group-title">恢复场景</h4>
      <p className="asb-scope-note">恢复前会先显示配置变更。场景只组合已有的供应商、扩展和指令，不会创建第二套配置。</p>
      {selected
        ? <div className="asb-project-plan-action-grid">
            <Button className="asb-project-plan-apply-action" variant="primary" disabled={busy} onClick={() => void run(async () => {
              clearOutcome();
              setPreview(await api.previewCodexProjectPlanApply(selected.id, view.revision));
            })}>预览并恢复场景</Button>
            <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
              reset(await api.resnapshotCodexProjectPlan(selected.id, view.revision));
              changed("已用当前工作状态更新场景。");
            })}>用当前状态更新场景</Button>
            <Button variant="secondary" disabled={busy} onClick={setCurrent}>{isCurrent ? "取消当前场景标记" : "设为当前场景"}</Button>
          </div>
        : <p className="asb-scope-note">先保存或选择一个场景，再进行恢复和维护。</p>}
    </section>
    {selected && <section className="asb-client-management-group asb-project-plan-delete" aria-label="删除工作场景">
      <h4 className="asb-group-title">删除场景</h4>
      <p className="asb-scope-note">删除不会修改 Codex 当前配置，但该工作场景无法恢复。</p>
      {deleting
        ? <div className="asb-project-plan-delete-confirmation" role="group" aria-label="确认删除工作场景">
            <p className="asb-warn-text">确认删除「{selected.name}」？此操作只删除保存的工作场景，不能恢复。</p>
            <div className="asb-form-actions">
              <Button variant="secondary" disabled={busy} onClick={() => setDeleting(false)}>保留场景</Button>
              <Button variant="danger" disabled={busy} onClick={() => void run(async () => {
                reset(await api.deleteCodexProjectPlan(selected.id, view.revision));
                setSelectedId(null); setName(""); setDeleting(false);
                changed("场景已删除，并已清除当前场景标记；尚未修改 Codex 配置。");
              })}>确认删除</Button>
            </div>
          </div>
        : <div className="asb-form-actions"><Button variant="danger" disabled={busy} onClick={() => setDeleting(true)}>删除当前场景</Button></div>}
    </section>}
  </>;
}
interface ProjectPlanOutcomeProps {
  view: api.CodexProjectPlansView;
  preview: api.CodexProjectApplyPreview | null;
  applied: string[];
  warnings: string[];
  operations: CodexOperations;
  setView: (view: api.CodexProjectPlansView) => void;
  setPreview: (preview: api.CodexProjectApplyPreview | null) => void;
  setApplied: (applied: string[]) => void;
  setWarnings: (warnings: string[]) => void;
}

function ProjectPlanOutcome({
  view,
  preview,
  applied,
  warnings,
  operations,
  setView,
  setPreview,
  setApplied,
  setWarnings,
}: ProjectPlanOutcomeProps) {
  const { run, busy, changed } = operations;
  const nameOf = (id: string) => projectPlanName(view, id);

  return <>
    {preview && <section className="asb-client-management-group" aria-label="Codex 工作场景恢复预览">
      <h4 className="asb-group-title">恢复预览</h4>
      <p className="asb-scope-note">{preview.autosavePlanId
        ? `恢复前会先将当前工作状态保存到「${nameOf(preview.autosavePlanId)}」。`
        : "没有当前场景需要保存。"}</p>
      {preview.steps.length
        ? <ul>{preview.steps.map((step, index) => <li key={`${step.kind}-${step.target}-${index}`}>{stepText(step)}</li>)}</ul>
        : <p className="asb-scope-note">该工作场景已与当前状态一致，无需写入配置。</p>}
      {preview.warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
      <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>返回场景</Button>
        <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
          const outcome = await api.applyCodexProjectPlan(preview.planId, view.revision, true);
          setView(outcome.view); setPreview(null);
          setApplied(outcome.steps.map(stepText)); setWarnings(outcome.warnings);
          changed(outcome.warnings.length
            ? "场景已恢复，但部分项目未完成；请查看下方提示。"
            : "场景已恢复，当前场景标记已更新。");
        })}>确认恢复场景</Button>
      </div>
    </section>}
    {applied.length > 0 && <section className="asb-client-management-group" aria-label="Codex 工作场景恢复结果">
      <h4 className="asb-group-title">恢复结果</h4>
      {applied.map((entry) => <p key={entry} className="asb-scope-note">{entry}</p>)}
    </section>}
    {warnings.map((warning) => <p key={warning} className="asb-warn-text">{warning}</p>)}
  </>;
}

/** Creates, maintains, previews, and applies Codex project snapshots. */
export function ProjectPlansPane({ operations }: { operations: CodexOperations }) {
  const { run, busy } = operations;
  const [view, setView] = useState<api.CodexProjectPlansView | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [newName, setNewName] = useState("");
  const [preview, setPreview] = useState<api.CodexProjectApplyPreview | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [applied, setApplied] = useState<string[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const description = "保存和恢复当前 Codex 工作场景。工作场景只记录已有供应商、扩展与指令的组合状态，不创建第二套配置；恢复时仍通过各功能原有的可恢复流程执行。";
  const reload = () => void run(async () => setView(await api.listCodexProjectPlans()));
  const clearOutcome = () => { setApplied([]); setWarnings([]); };
  const reset = (next: api.CodexProjectPlansView) => {
    setView(next); setPreview(null); clearOutcome();
  };

  useEffect(() => { void run(async () => setView(await api.listCodexProjectPlans())); }, [run]);

  if (!view) return (
    <ClientManagementModule title="工作场景" description={description} refreshLabel="重新读取场景" busy={busy} onRefresh={reload}>
      <p role="status">正在读取工作场景…</p>
    </ClientManagementModule>
  );

  return <ClientManagementModule title="工作场景" description={description} refreshLabel="重新读取场景" busy={busy} onRefresh={reload}>
    <ProjectPlanDetails
      view={view}
      operations={operations}
      selectedId={selectedId}
      name={name}
      newName={newName}
      setSelectedId={setSelectedId}
      setName={setName}
      setNewName={setNewName}
      setPreview={setPreview}
      setDeleting={setDeleting}
      clearOutcome={clearOutcome}
      reset={reset}
    />
    <ProjectPlanActions
      view={view}
      operations={operations}
      selectedId={selectedId}
      deleting={deleting}
      setSelectedId={setSelectedId}
      setName={setName}
      setPreview={setPreview}
      setDeleting={setDeleting}
      clearOutcome={clearOutcome}
      reset={reset}
    />
    <ProjectPlanOutcome
      view={view}
      preview={preview}
      applied={applied}
      warnings={warnings}
      operations={operations}
      setView={setView}
      setPreview={setPreview}
      setApplied={setApplied}
      setWarnings={setWarnings}
    />
  </ClientManagementModule>;
}
