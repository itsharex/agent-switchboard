import { uiMessage } from "../../i18n/errors";
import { useEffect, useId, useState } from "react";
import * as api from "../../api/codex-project-plans";
import type { MessageKey, TFunction } from "../../i18n";
import { useI18n } from "../../i18n";
import { localizedMessageText } from "../../i18n/errors";
import { Button } from "../Button";
import { Input } from "../Input";
import { ClientManagementModule } from "../client-management/ClientManagementModule";
import type { CodexOperations } from "./operations";

const ACTION_LABELS: Record<api.CodexProjectApplyStep["action"], MessageKey> = {
  switch: "codex.plans.actionSwitch", enable: "codex.plans.actionEnable",
  disable: "codex.plans.actionDisable", activate: "codex.plans.actionActivate",
};
const KIND_LABELS: Record<api.CodexProjectApplyStep["kind"], MessageKey> = {
  provider: "codex.plans.kindProvider", mcp: "codex.plans.kindMcp",
  skill: "codex.plans.kindSkill", prompt: "codex.plans.kindPrompt",
};

function stepText(step: api.CodexProjectApplyStep, t: TFunction): string {
  return t("codex.plans.stepText", {
    kind: t(KIND_LABELS[step.kind]),
    action: t(ACTION_LABELS[step.action]),
    label: step.label,
  });
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
  const { t } = useI18n();
  const selected = view.plans.find((plan) => plan.id === selectedId) ?? null;
  const nameOf = (id: string) => projectPlanName(view, id);
  const ledgerName = useId();
  const select = (id: string | null) => {
    const plan = id === null ? null : view.plans.find((item) => item.id === id) ?? null;
    setSelectedId(plan?.id ?? null); setName(plan?.name ?? "");
    setPreview(null); setDeleting(false); clearOutcome();
  };

  return <section className="asb-client-management-group" aria-label={t("codex.plans.contentAria")}>
    <h4 className="asb-group-title">{t("codex.plans.groupScenario")}</h4>
    <p className="asb-scope-note">{t("codex.plans.statusLine", {
      provider: view.activeProviderId ? nameOf(view.activeProviderId) : t("codex.plans.none"),
      mcp: view.bindings.filter((binding) => binding.kind === "mcp" && binding.enabled).length,
      skills: view.bindings.filter((binding) => binding.kind === "skill" && binding.enabled).length,
      prompt: view.activePromptId ? nameOf(view.activePromptId) : t("codex.plans.none"),
    })}</p>
    <div role="radiogroup" aria-label={t("codex.plans.selectAria")} className="asb-client-management-ledger">
      <label className={"asb-client-management-option" + (!selectedId ? " is-active" : "")}>
        <input type="radio" name={ledgerName} checked={!selectedId} disabled={busy} onChange={() => select(null)} />
        <span className="asb-client-management-option-text">
          <span className="asb-client-management-option-name">{t("codex.plans.newScenario")}</span>
          <span className="asb-client-management-option-meta">{t("codex.plans.newScenarioMeta")}</span>
        </span>
      </label>
      {view.plans.map((plan) => (
        <label key={plan.id} className={"asb-client-management-option" + (plan.id === selectedId ? " is-active" : "")}>
          <input type="radio" name={ledgerName} checked={plan.id === selectedId} disabled={busy}
            onChange={() => select(plan.id)} />
          <span className="asb-client-management-option-text">
            <span className="asb-client-management-option-name">{plan.name}</span>
            <span className="asb-client-management-option-meta">{api.describeCodexProjectSlot(plan.slot, nameOf)}</span>
          </span>
          {plan.id === view.current && <span className="asb-status-pill is-ok">
            <span className="asb-status-pill-dot" aria-hidden="true" />{t("codex.plans.currentPill")}
          </span>}
        </label>
      ))}
    </div>
    {selected
      ? <label className="asb-field is-medium"><span>{t("codex.plans.nameLabel")}</span><Input value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
      : <label className="asb-field is-medium"><span>{t("codex.plans.newNameLabel")}</span><Input value={newName} disabled={busy} onChange={(event) => setNewName(event.target.value)} /></label>}
    <div className="asb-form-actions">
      {selected
        ? <Button variant="primary" disabled={busy || !name.trim()} onClick={() => void run(async () => {
            reset(await api.renameCodexProjectPlan(selected.id, name, view.revision));
            changed(uiMessage("codex.plans.nameSaved"));
          })}>{t("codex.plans.saveName")}</Button>
        : <Button variant="primary" disabled={busy || !newName.trim()} onClick={() => void run(async () => {
            const next = await api.createCodexProjectPlan(newName, view.revision);
            reset(next);
            const created = next.plans[next.plans.length - 1] ?? null;
            setSelectedId(created?.id ?? null); setName(created?.name ?? ""); setNewName("");
            changed(uiMessage("codex.plans.currentSaved"));
          })}>{t("codex.plans.saveCurrent")}</Button>}
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
  const { t } = useI18n();
  const selected = view.plans.find((plan) => plan.id === selectedId) ?? null;
  const isCurrent = !!selected && selected.id === view.current;

  const setCurrent = () => void run(async () => {
    if (!selected) return;
    reset(await api.setCurrentCodexProjectPlan(isCurrent ? null : selected.id, view.revision));
    changed(isCurrent ? uiMessage("codex.plans.clearedCurrent") : uiMessage("codex.plans.setCurrentDone"));
  });

  return <>
    <section className="asb-client-management-group asb-project-plan-actions" aria-label={t("codex.plans.actionsAria")}>
      <h4 className="asb-group-title">{t("codex.plans.restoreGroup")}</h4>
      <p className="asb-scope-note">{t("codex.plans.restoreNote")}</p>
      {selected
        ? <div className="asb-project-plan-action-grid">
            <Button className="asb-project-plan-apply-action" variant="primary" disabled={busy} onClick={() => void run(async () => {
              clearOutcome();
              setPreview(await api.previewCodexProjectPlanApply(selected.id, view.revision));
            })}>{t("codex.plans.previewApply")}</Button>
            <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
              reset(await api.resnapshotCodexProjectPlan(selected.id, view.revision));
              changed(uiMessage("codex.plans.updatedFromCurrent"));
            })}>{t("codex.plans.updateFromCurrent")}</Button>
            <Button variant="secondary" disabled={busy} onClick={setCurrent}>{isCurrent ? t("codex.plans.clearCurrent") : t("codex.plans.setCurrentLabel")}</Button>
          </div>
        : <p className="asb-scope-note">{t("codex.plans.selectFirst")}</p>}
    </section>
    {selected && <section className="asb-client-management-group asb-project-plan-delete" aria-label={t("codex.plans.deleteGroupAria")}>
      <h4 className="asb-group-title">{t("codex.plans.deleteGroup")}</h4>
      <p className="asb-scope-note">{t("codex.plans.deleteNote")}</p>
      {deleting
        ? <div className="asb-project-plan-delete-confirmation" role="group" aria-label={t("codex.plans.deleteConfirmAria")}>
            <p className="asb-warn-text">{t("codex.plans.deleteConfirm", { name: selected.name })}</p>
            <div className="asb-form-actions">
              <Button variant="secondary" disabled={busy} onClick={() => setDeleting(false)}>{t("codex.plans.keep")}</Button>
              <Button variant="danger" disabled={busy} onClick={() => void run(async () => {
                reset(await api.deleteCodexProjectPlan(selected.id, view.revision));
                setSelectedId(null); setName(""); setDeleting(false);
                changed(uiMessage("codex.plans.deleted"));
              })}>{t("codex.plans.confirmDelete")}</Button>
            </div>
          </div>
        : <div className="asb-form-actions"><Button variant="danger" disabled={busy} onClick={() => setDeleting(true)}>{t("codex.plans.deleteCurrent")}</Button></div>}
    </section>}
  </>;
}
interface ProjectPlanOutcomeProps {
  view: api.CodexProjectPlansView;
  preview: api.CodexProjectApplyPreview | null;
  applied: api.CodexProjectApplyStep[];
  warnings: api.CodexProjectApplyOutcome["warnings"];
  operations: CodexOperations;
  setView: (view: api.CodexProjectPlansView) => void;
  setPreview: (preview: api.CodexProjectApplyPreview | null) => void;
  setApplied: (applied: api.CodexProjectApplyStep[]) => void;
  setWarnings: (warnings: api.CodexProjectApplyOutcome["warnings"]) => void;
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
  const { t } = useI18n();
  const nameOf = (id: string) => projectPlanName(view, id);

  return <>
    {preview && <section className="asb-client-management-group" aria-label={t("codex.plans.previewAria")}>
      <h4 className="asb-group-title">{t("codex.plans.previewGroup")}</h4>
      <p className="asb-scope-note">{preview.autosavePlanId
        ? t("codex.plans.autosaveNote", { name: nameOf(preview.autosavePlanId) })
        : t("codex.plans.noAutosave")}</p>
      {preview.steps.length
        ? <ul>{preview.steps.map((step, index) => <li key={`${step.kind}-${step.target}-${index}`}>{stepText(step, t)}</li>)}</ul>
        : <p className="asb-scope-note">{t("codex.plans.uptodate")}</p>}
      {preview.warnings.map((warning) => <p key={warning.key} className="asb-warn-text">{localizedMessageText(warning, t)}</p>)}
      <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>{t("codex.plans.backToScenarios")}</Button>
        <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
          const outcome = await api.applyCodexProjectPlan(preview.planId, view.revision, true);
          setView(outcome.view); setPreview(null);
          setApplied(outcome.steps); setWarnings(outcome.warnings);
          changed(outcome.warnings.length
            ? uiMessage("codex.plans.appliedWithWarnings")
            : uiMessage("codex.plans.appliedClean"));
        })}>{t("codex.plans.confirmApply")}</Button>
      </div>
    </section>}
    {applied.length > 0 && <section className="asb-client-management-group" aria-label={t("codex.plans.resultAria")}>
      <h4 className="asb-group-title">{t("codex.plans.resultGroup")}</h4>
      {applied.map((entry, index) => <p key={`${entry.kind}-${entry.target}-${index}`} className="asb-scope-note">{stepText(entry, t)}</p>)}
    </section>}
    {warnings.map((warning) => <p key={warning.key} className="asb-warn-text">{localizedMessageText(warning, t)}</p>)}
  </>;
}

/** Creates, maintains, previews, and applies Codex project snapshots. */
export function ProjectPlansPane({ operations }: { operations: CodexOperations }) {
  const { run, busy } = operations;
  const { t } = useI18n();
  const [view, setView] = useState<api.CodexProjectPlansView | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [newName, setNewName] = useState("");
  const [preview, setPreview] = useState<api.CodexProjectApplyPreview | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [applied, setApplied] = useState<api.CodexProjectApplyStep[]>([]);
  const [warnings, setWarnings] = useState<api.CodexProjectApplyOutcome["warnings"]>([]);
  const description = t("codex.plans.description");
  const reload = () => void run(async () => setView(await api.listCodexProjectPlans()));
  const clearOutcome = () => { setApplied([]); setWarnings([]); };
  const reset = (next: api.CodexProjectPlansView) => {
    setView(next); setPreview(null); clearOutcome();
  };

  useEffect(() => { void run(async () => setView(await api.listCodexProjectPlans())); }, [run]);

  if (!view) return (
    <ClientManagementModule title={t("codex.plans.title")} description={description} refreshLabel={t("codex.plans.refresh")} busy={busy} onRefresh={reload}>
      <div className="asb-client-management-skeleton" role="status" aria-label={t("codex.loading")}>
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
      </div>
    </ClientManagementModule>
  );

  return <ClientManagementModule title={t("codex.plans.title")} description={description} refreshLabel={t("codex.plans.refresh")} busy={busy} onRefresh={reload}>
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
