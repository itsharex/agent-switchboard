import { CUSTOM_PROBE_QUESTION_ID, type CodexProbeBatch } from "../api/client";
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

function progressLine(batch: CodexProbeBatch): string {
  const summary = summarizeRuns(batch.runs);
  switch (batch.status) {
    case "running":
      return batch.completedRuns < batch.plannedRuns
        ? `正在执行第 ${batch.completedRuns + 1}/${batch.plannedRuns} 次（已完成 ${batch.completedRuns} 次）…`
        : "正在结束本次检测…";
    case "completed":
      return `检测完成：${summary.passedCount}/${summary.judgedCount} 次通过。`;
    case "cancelled":
      return "本次检测已取消。";
    case "failed":
      return "本次检测失败。";
    case "interrupted":
      return "应用退出时检测未完成，已保留以下已完成结果；重新检测会开始一个新批次。";
    case "config-changed":
      return "检测期间当前 Codex 配置发生变化，当前调用未判定，已停止后续调用。";
  }
}

function CustomQuestionForm({ form, disabled }: { form: CodexProbeFormState; disabled: boolean }) {
  if (form.questionId !== CUSTOM_PROBE_QUESTION_ID) return null;
  return (
    <div className="asb-codex-probe-custom">
      <label htmlFor="codex-probe-custom-question">自定义题目</label>
      <Textarea id="codex-probe-custom-question" rows={3} value={form.customQuestion}
        placeholder="输入题目；检测时会要求模型只输出最终非负整数"
        disabled={disabled} onChange={(event) => form.onCustomQuestionChange(event.target.value)} />
      <label htmlFor="codex-probe-custom-answer">期望答案（非负整数）</label>
      <Input id="codex-probe-custom-answer" placeholder="例如 21" value={form.customAnswer}
        disabled={disabled} onChange={(event) => form.onCustomAnswerChange(event.target.value)} />
    </div>
  );
}

function ProbeResults({ status, probe }: {
  status: CodexProbeBatch;
  probe: ReturnType<typeof useCodexProbe>;
}) {
  const summary = summarizeRuns(status.runs);
  const heading = status.status === "running" ? "当前检测" : "最近一次检测";
  return (
    <div className="asb-codex-probe-content">
      <p className="asb-codex-probe-progress" role="status" aria-live="polite">{progressLine(status)}</p>
      {status.status !== "running" && status.statusError &&
        <p className="asb-warn-text" role="alert">{status.statusError}</p>}
      {status.persistPending && <p className="asb-warn-text" role="alert">
        {status.persistError ?? "部分检测结果尚未保存到本地历史。"}
        {" "}
        <Button variant="secondary" disabled={probe.retrying}
          onClick={() => void probe.retrySave()}>
          {probe.retrying ? "正在重试保存…" : "重试保存"}
        </Button>
      </p>}
      {summary.runCount > 0 && <>
        <div role="group" aria-label="检测结果汇总"><StatCards stats={probeSummaryCards(status)} /></div>
        {summary.recordedRuns < summary.runCount && <p className="asb-codex-probe-note" role="status">
          仅 {summary.recordedRuns}/{summary.runCount} 次取得总消耗记录；缺失用量未知，汇总不代表完整消耗。
        </p>}
        <ProbeRunsTable batch={status} />
      </>}
      {(status.status === "cancelled" || status.status === "interrupted") && <p className="asb-codex-probe-note">
        被取消或中断的调用可能已消耗额度；本页汇总仅包含已完成的运行记录。
      </p>}
      <p className="asb-codex-probe-meta">
        {heading} · 题目：{status.question.label || "—"} · 开始于 <Time iso={status.startedAt} />
        {status.finishedAt && <> · 结束于 <Time iso={status.finishedAt} /></>}
      </p>
      <p className="asb-codex-probe-meta">
        当时配置：{configSummary(status)}
        {status.cliVersion && ` · CLI ${status.cliVersion}`}
      </p>
    </div>
  );
}

export function CodexProbePanel({ probe, form }: {
  probe: ReturnType<typeof useCodexProbe>;
  form: CodexProbeFormState;
}) {
  return (
    <section className="asb-panel asb-codex-probe" aria-label="降智雷达">
      <ModuleHeader title="降智雷达" />
      {probe.catalogError && <p className="asb-warn-text" role="alert">题目列表不可用：{probe.catalogError}</p>}
      {probe.startError && <p className="asb-warn-text" role="alert">无法开始检测：{probe.startError}</p>}
      {probe.readError && <p className="asb-warn-text" role="alert">
        状态读取失败：{probe.readError}{" "}
        <Button variant="secondary" disabled={probe.retrying} onClick={() => void probe.retrySave()}>
          {probe.retrying ? "正在重试…" : "重试保存并读取"}
        </Button>
      </p>}
      {probe.cancelError && <p className="asb-warn-text" role="alert">无法取消检测：{probe.cancelError}</p>}
      {probe.retryError && <p className="asb-warn-text" role="alert">无法重试保存：{probe.retryError}</p>}
      <CustomQuestionForm form={form} disabled={probe.running || probe.starting} />
      {probe.status ? <ProbeResults status={probe.status} probe={probe} /> : (
        <div className="asb-empty-state asb-codex-probe-empty">
          <span className="asb-empty-state-icon" aria-hidden="true"><UsageIcon /></span>
          <h3 className="asb-section-title" role="status">
            {probe.running ? "正在读取检测进度…" : probe.starting ? "正在启动检测…"
              : "尚未开始检测。每次检测都会真实调用当前激活的 Codex 配置并消耗额度。"}
          </h3>
        </div>
      )}
      <p className="asb-codex-probe-note">
        仅按最终非负整数答案判分；运行失败不算通过。检测会消耗额度并计入「消耗统计」；自定义题目与最终回答会保存在本机检测历史中。单题结果存在方差，持续的低通过率只是参考信号，不构成模型判定。
      </p>
    </section>
  );
}
