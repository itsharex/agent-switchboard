import { CUSTOM_PROBE_QUESTION_ID, type CodexProbeBatch } from "../api/client";
import type { TFunction } from "../i18n";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { Input } from "./Input";
import {
  ProbeRunsTable, configSummary, probeSummaryCards, summarizeRuns,
} from "./probe-shared";
import { Textarea } from "./Textarea";
import { Time } from "./Time";
import { ModuleHeader } from "./WorkspaceHeader";
import { UsageIcon } from "./icons";
import { StatCards } from "./charts/stat-cards";
import type { useCodexProbe } from "./use-codex-probe";

/** The custom-question form lives with the panel because only a `custom`
 * selection needs it; the header keeps the action and the pickers. */
export interface CodexProbeFormState {
  questionId: string | null;
  customQuestion: string;
  customAnswer: string;
  onCustomQuestionChange: (value: string) => void;
  onCustomAnswerChange: (value: string) => void;
}

function progressLine(batch: CodexProbeBatch, t: TFunction): string {
  const summary = summarizeRuns(batch.runs);
  switch (batch.status) {
    case "running":
      return batch.completedRuns < batch.plannedRuns
        ? t("codex.probe.progressRun", {
          current: batch.completedRuns + 1, total: batch.plannedRuns, done: batch.completedRuns,
        })
        : t("codex.probe.finishing");
    case "completed":
      return t("codex.probe.completedLine", { passed: summary.passedCount, judged: summary.judgedCount });
    case "cancelled":
      return t("codex.probe.cancelledLine");
    case "failed":
      return t("codex.probe.failedLine");
    case "interrupted":
      return t("codex.probe.interruptedLine");
    case "config-changed":
      return t("codex.probe.configChangedLine");
  }
}

function CustomQuestionForm({ form, disabled }: { form: CodexProbeFormState; disabled: boolean }) {
  const { t } = useI18n();
  if (form.questionId !== CUSTOM_PROBE_QUESTION_ID) return null;
  return (
    <div className="asb-codex-probe-custom">
      <label htmlFor="codex-probe-custom-question">{t("codex.probe.customQuestion")}</label>
      <Textarea id="codex-probe-custom-question" rows={3} value={form.customQuestion}
        placeholder={t("codex.probe.customQuestionPlaceholder")}
        disabled={disabled} onChange={(event) => form.onCustomQuestionChange(event.target.value)} />
      <label htmlFor="codex-probe-custom-answer">{t("codex.probe.expectedAnswer")}</label>
      <Input id="codex-probe-custom-answer" placeholder={t("codex.probe.exampleAnswer")} value={form.customAnswer}
        disabled={disabled} onChange={(event) => form.onCustomAnswerChange(event.target.value)} />
    </div>
  );
}

function ProbeResults({ status, probe }: {
  status: CodexProbeBatch;
  probe: ReturnType<typeof useCodexProbe>;
}) {
  const { t } = useI18n();
  const summary = summarizeRuns(status.runs);
  const heading = status.status === "running" ? t("codex.probe.currentHeading") : t("codex.probe.lastHeading");
  return (
    <div className="asb-codex-probe-content">
      <p className="asb-codex-probe-progress" role="status" aria-live="polite">{progressLine(status, t)}</p>
      {status.status !== "running" && status.statusError &&
        <p className="asb-warn-text" role="alert">{status.statusError}</p>}
      {status.persistPending && <p className="asb-warn-text" role="alert">
        {status.persistError ?? t("codex.probe.persistPending")}
        {" "}
        <Button variant="secondary" disabled={probe.retrying}
          onClick={() => void probe.retrySave()}>
          {probe.retrying ? t("codex.probe.retryingSave") : t("codex.probe.retrySave")}
        </Button>
      </p>}
      {summary.runCount > 0 && <>
        <div role="group" aria-label={t("codex.probe.summaryAria")}><StatCards stats={probeSummaryCards(status, t)} /></div>
        {summary.recordedRuns < summary.runCount && <p className="asb-codex-probe-note" role="status">
          {t("codex.probe.partialUsage", { recorded: summary.recordedRuns, total: summary.runCount })}
        </p>}
        <ProbeRunsTable batch={status} />
      </>}
      {(status.status === "cancelled" || status.status === "interrupted") && <p className="asb-codex-probe-note">
        {t("codex.probe.cancelledNote")}
      </p>}
      <p className="asb-codex-probe-meta">
        {heading} · {t("codex.probe.metaQuestion")}{status.question.label || "—"} · {t("codex.probe.startedAt")} <Time iso={status.startedAt} />
        {status.finishedAt && <> · {t("codex.probe.finishedAt")} <Time iso={status.finishedAt} /></>}
      </p>
      <p className="asb-codex-probe-meta">
        {t("codex.probe.configPrefix")}{configSummary(status, t)}
        {status.cliVersion && ` · CLI ${status.cliVersion}`}
      </p>
    </div>
  );
}

export function CodexProbePanel({ probe, form }: {
  probe: ReturnType<typeof useCodexProbe>;
  form: CodexProbeFormState;
}) {
  const { t } = useI18n();
  return (
    <section className="asb-panel asb-codex-probe" aria-label={t("codex.probe.title")}>
      <ModuleHeader title={t("codex.probe.title")} />
      {probe.catalogError && <p className="asb-warn-text" role="alert">{t("codex.probe.catalogError", { error: probe.catalogError })}</p>}
      {probe.startError && <p className="asb-warn-text" role="alert">{t("codex.probe.startError", { error: probe.startError })}</p>}
      {probe.readError && <p className="asb-warn-text" role="alert">
        {t("codex.probe.readError", { error: probe.readError })}{" "}
        <Button variant="secondary" disabled={probe.retrying} onClick={() => void probe.retrySave()}>
          {probe.retrying ? t("codex.probe.retrying") : t("codex.probe.retrySaveAndRead")}
        </Button>
      </p>}
      {probe.cancelError && <p className="asb-warn-text" role="alert">{t("codex.probe.cancelError", { error: probe.cancelError })}</p>}
      {probe.retryError && <p className="asb-warn-text" role="alert">{t("codex.probe.retryError", { error: probe.retryError })}</p>}
      <CustomQuestionForm form={form} disabled={probe.running || probe.starting} />
      {probe.status ? <ProbeResults status={probe.status} probe={probe} /> : (
        <div className="asb-empty-state asb-codex-probe-empty">
          <span className="asb-empty-state-icon" aria-hidden="true"><UsageIcon /></span>
          <h3 className="asb-section-title" role="status">
            {probe.running ? t("codex.probe.readingProgress") : probe.starting ? t("codex.probe.starting")
              : t("codex.probe.notStarted")}
          </h3>
        </div>
      )}
      <p className="asb-codex-probe-note">
        {t("codex.probe.note")}
      </p>
    </section>
  );
}
