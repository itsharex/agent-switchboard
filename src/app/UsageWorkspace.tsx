import { useEffect, useId, useState } from "react";
import { CUSTOM_PROBE_QUESTION_ID, type ModelUsageRange } from "../api/client";
import { CodexOfficialResetPanel } from "../components/CodexOfficialResetPanel";
import { CodexProbeHistory } from "../components/CodexProbeHistory";
import { CodexProbePanel, type CodexProbeFormState } from "../components/CodexProbePanel";
import { CodexResetPanel } from "../components/CodexResetPanel";
import { useCodexOfficialReset, useCodexResetSignal } from "../components/quota-reads";
import { useCodexProbe } from "../components/use-codex-probe";
import { useI18n } from "../i18n";
import { UsagePage } from "../pages/UsagePage";
import { useModelUsageReport } from "../pages/use-model-usage-report";
import { UsageWorkspaceHeader } from "./UsageWorkspaceHeader";
import type { UsageSection } from "./navigation";

export function UsageWorkspace({ active, section, onSectionChange }: {
  active: boolean;
  section: UsageSection;
  onSectionChange: (section: UsageSection) => void;
}) {
  const { t } = useI18n();
  const id = useId();
  const [quotaOpened, setQuotaOpened] = useState(section === "quota");
  const [radarOpened, setRadarOpened] = useState(section === "radar");
  const [historyOpened, setHistoryOpened] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [range, setRange] = useState<ModelUsageRange>("last7Days");
  useEffect(() => { if (section === "quota") setQuotaOpened(true); }, [section]);
  useEffect(() => { if (section === "radar") setRadarOpened(true); }, [section]);
  const consumptionActive = active && section === "consumption";
  const usage = useModelUsageReport(consumptionActive, range);
  // Polling stays on while the history route is open too: its header keeps a
  // live entry to a running batch.
  const radarActive = active && section === "radar";
  const probe = useCodexProbe(radarActive);
  const [questionId, setQuestionId] = useState<string | null>(null);
  const [runCount, setRunCount] = useState(3);
  const [customQuestion, setCustomQuestion] = useState("");
  const [customAnswer, setCustomAnswer] = useState("");
  useEffect(() => {
    if (questionId === null && probe.questions && probe.questions.length > 0) {
      setQuestionId(probe.questions[0].id);
    }
  }, [questionId, probe.questions]);
  const form: CodexProbeFormState = { questionId, customQuestion, customAnswer,
    onCustomQuestionChange: setCustomQuestion, onCustomAnswerChange: setCustomAnswer };
  /** Fills the radar form from a historical question; the start action still
   * belongs to the user, and the new batch runs against the current
   * configuration. */
  const reuseQuestion = (question: { id: string; text: string; expectedAnswer: string },
    plannedRuns: number) => {
    setQuestionId(CUSTOM_PROBE_QUESTION_ID);
    setCustomQuestion(question.text);
    setCustomAnswer(question.expectedAnswer);
    setRunCount(plannedRuns);
    setHistoryOpen(false);
  };
  const quotaReadsVisible = quotaOpened || section === "quota";
  const officialReset = useCodexOfficialReset(quotaReadsVisible);
  const resetSignal = useCodexResetSignal(quotaReadsVisible);
  return (
    <section className="asb-page-stack" aria-label={t("usage.workspace.aria")}>
      <div hidden={historyOpen}>
        <UsageWorkspaceHeader id={id} section={section} onSectionChange={onSectionChange}
          consumption={{ active: consumptionActive, range, onRangeChange: setRange, usage }}
          officialReset={officialReset} resetSignal={resetSignal}
          radar={{ probe, form, onQuestionChange: setQuestionId,
            runCount, onRunCountChange: setRunCount,
            onOpenHistory: () => { setHistoryOpened(true); setHistoryOpen(true); } }} />
        <div id={`${id}-consumption-panel`} role="tabpanel" aria-labelledby={`${id}-consumption-tab`} hidden={section !== "consumption"}>
          <UsagePage usage={usage} />
        </div>
        <div id={`${id}-quota-panel`} role="tabpanel" aria-labelledby={`${id}-quota-tab`} hidden={section !== "quota"}>
          {quotaReadsVisible && <div className="asb-page-stack">
            <CodexOfficialResetPanel read={officialReset} />
            <CodexResetPanel read={resetSignal} />
          </div>}
        </div>
        <div id={`${id}-radar-panel`} role="tabpanel" aria-labelledby={`${id}-radar-tab`} hidden={section !== "radar"}>
          {radarOpened && <CodexProbePanel probe={probe} form={form} />}
        </div>
      </div>
      {historyOpened && (
        <div className="asb-editor-route" hidden={!historyOpen}>
          <CodexProbeHistory active={radarActive && historyOpen} probe={probe} onClose={() => setHistoryOpen(false)}
            onReuseQuestion={reuseQuestion} onChanged={probe.refresh} />
        </div>
      )}
    </section>
  );
}
