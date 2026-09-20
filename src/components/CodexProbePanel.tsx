import { CUSTOM_PROBE_QUESTION_ID, type CodexProbeRun, type CodexProbeStatus } from "../api/client";
import { formatCompactTokenCount, formatTokenCount, TOKEN_UNIT } from "../lib/token-format";
import { Input } from "./Input";
import { Table, type TableColumn } from "./Table";
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

function formatDuration(totalMs: number): string {
  const seconds = Math.round(totalMs / 1000);
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const restSeconds = seconds % 60;
  if (minutes < 60) return restSeconds ? `${minutes} 分 ${restSeconds} 秒` : `${minutes} 分`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes ? `${hours} 时 ${restMinutes} 分` : `${hours} 时`;
}

interface ProbeRunRow extends CodexProbeRun {
  index: number;
}

const RESULT_CLASS = { ok: "asb-ok-text", fail: "asb-fail-text" } as const;

const RUN_COLUMNS: Array<TableColumn<ProbeRunRow>> = [
  {
    key: "index",
    header: "序号",
    render: (run) => `${run.index}`,
  },
  {
    key: "result",
    header: "结果",
    render: (run) =>
      run.passed ? (
        <span className={RESULT_CLASS.ok}>通过</span>
      ) : (
        <span className={RESULT_CLASS.fail}>{run.error ? `未通过 · ${run.error}` : "未通过"}</span>
      ),
  },
  {
    key: "reasoning",
    header: `Reasoning（${TOKEN_UNIT}）`,
    render: (run) =>
      run.reasoningTokens === null ? "—" : formatTokenCount(run.reasoningTokens),
  },
  {
    key: "total",
    header: `总计（${TOKEN_UNIT}）`,
    render: (run) => (run.totalTokens === null ? "—" : formatTokenCount(run.totalTokens)),
  },
  {
    key: "model",
    header: "模型",
    cellClassName: "asb-code",
    render: (run) => run.model ?? "—",
  },
  {
    key: "duration",
    header: "耗时",
    render: (run) => formatDuration(run.durationMs),
  },
];

function summaryCards(status: CodexProbeStatus) {
  const completed = status.runs;
  const passedCount = completed.filter((run) => run.passed).length;
  const reasoningRuns = completed.filter((run) => run.reasoningTokens !== null);
  const averageReasoning = reasoningRuns.length
    ? Math.round(
        reasoningRuns.reduce((total, run) => total + (run.reasoningTokens ?? 0), 0) /
          reasoningRuns.length,
      )
    : null;
  const totalTokens = completed.reduce((total, run) => total + (run.totalTokens ?? 0), 0);
  const totalMs = completed.reduce((total, run) => total + run.durationMs, 0);
  return [
    { label: "通过", value: `${passedCount}/${completed.length}` },
    {
      label: "平均 reasoning",
      value: averageReasoning === null ? "—" : formatCompactTokenCount(averageReasoning),
      unit: TOKEN_UNIT,
    },
    { label: "实测消耗", value: formatCompactTokenCount(totalTokens), unit: TOKEN_UNIT },
    { label: "总耗时", value: formatDuration(totalMs) },
  ];
}

/** The degradation radar: an opt-in local probe of the active Codex
 * configuration. Each batch is a set of real Codex calls that spends quota
 * and lands in the consumption report; results are reference signals, not
 * verdicts. The start and cancel actions live in the usage page header. */
export function CodexProbePanel({ probe, form }: {
  probe: ReturnType<typeof useCodexProbe>;
  form: CodexProbeFormState;
}) {
  const { status } = probe;
  const rows: ProbeRunRow[] = status
    ? status.runs.map((run, index) => ({ ...run, index: index + 1 }))
    : [];

  return (
    <section className="asb-panel asb-codex-probe" aria-label="降智雷达">
      <ModuleHeader title="降智雷达" />
      {probe.catalogError && (
        <p className="asb-warn-text" role="alert">题目列表不可用：{probe.catalogError}</p>
      )}
      {probe.startError && (
        <p className="asb-warn-text" role="alert">无法开始检测：{probe.startError}</p>
      )}
      {form.questionId === CUSTOM_PROBE_QUESTION_ID && (
        <div className="asb-codex-probe-custom">
          <label htmlFor="codex-probe-custom-question">自定义题目</label>
          <Textarea
            id="codex-probe-custom-question"
            rows={3}
            placeholder="粘贴你想重复测试的题目（例如社区常用的对照题）"
            value={form.customQuestion}
            disabled={probe.running}
            onChange={(event) => form.onCustomQuestionChange(event.target.value)}
          />
          <label htmlFor="codex-probe-custom-answer">期望答案（独立整数）</label>
          <Input
            id="codex-probe-custom-answer"
            placeholder="例如 21"
            value={form.customAnswer}
            disabled={probe.running}
            onChange={(event) => form.onCustomAnswerChange(event.target.value)}
          />
        </div>
      )}
      {status === null ? (
        <div className="asb-empty-state asb-codex-probe-empty">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UsageIcon />
          </span>
          <h3 className="asb-section-title">
            尚未开始检测。每次检测都会真实调用当前激活的 Codex 配置并消耗额度。
          </h3>
        </div>
      ) : (
        <div className="asb-codex-probe-content">
          <p className="asb-codex-probe-progress" role="status" aria-live="polite">
            {status.phase === "running"
              ? status.completedRuns < status.runCount
                ? `正在执行第 ${status.completedRuns + 1}/${status.runCount} 次（已完成 ${status.completedRuns} 次）…`
                : "正在结束本次检测…"
              : status.phase === "cancelled"
                ? "本次检测已取消。"
                : status.phase === "failed"
                  ? "本次检测失败。"
                  : `检测完成：${status.runs.filter((run) => run.passed).length}/${status.runs.length} 次通过。`}
          </p>
          {status.phase === "failed" && status.error && (
            <p className="asb-warn-text" role="alert">{status.error}</p>
          )}
          {rows.length > 0 && (
            <>
              <div role="group" aria-label="检测结果汇总">
                <StatCards stats={summaryCards(status)} />
              </div>
              <Table
                columns={RUN_COLUMNS}
                rows={rows}
                rowKey={(run) => `probe-run-${run.index}`}
                ariaLabel="降智雷达每次运行明细"
                className="asb-codex-probe-table"
              />
            </>
          )}
          <p className="asb-codex-probe-meta">
            题目：{status.questionLabel || "—"} · 开始于 <Time iso={status.startedAt} />
          </p>
        </div>
      )}
      <p className="asb-codex-probe-note">
        检测通过本机真实调用完成，会消耗额度并计入「消耗统计」；单题结果存在方差，持续的低通过率只是参考信号，不构成模型判定。
      </p>
    </section>
  );
}
