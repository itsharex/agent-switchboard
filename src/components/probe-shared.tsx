import type {
  CodexProbeBatch, CodexProbeBatchStatus, CodexProbeRun, CodexProbeRunStatus,
} from "../api/client";
import { formatCompactTokenCount, formatTokenCount, TOKEN_UNIT } from "../lib/token-format";
import { Table, type TableColumn } from "./Table";

export const BATCH_STATUS_LABEL: Record<CodexProbeBatchStatus, string> = {
  running: "进行中",
  completed: "已完成",
  cancelled: "已取消",
  failed: "失败",
  interrupted: "已中断",
  "config-changed": "配置已变化",
};

export const BATCH_STATUS_CLASS: Record<CodexProbeBatchStatus, string> = {
  running: "",
  completed: "asb-ok-text",
  cancelled: "",
  failed: "asb-fail-text",
  interrupted: "asb-warn-text",
  "config-changed": "asb-warn-text",
};

export const RUN_STATUS_LABEL: Record<CodexProbeRunStatus, string> = {
  running: "执行中",
  passed: "通过",
  failed: "未通过",
  undetermined: "未判定",
};

export const RUN_STATUS_CLASS: Record<CodexProbeRunStatus, string> = {
  running: "",
  passed: "asb-ok-text",
  failed: "asb-fail-text",
  undetermined: "asb-warn-text",
};

export function formatDuration(totalMs: number): string {
  const seconds = Math.round(totalMs / 1000);
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const restSeconds = seconds % 60;
  if (minutes < 60) return restSeconds ? `${minutes} 分 ${restSeconds} 秒` : `${minutes} 分`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes ? `${hours} 时 ${restMinutes} 分` : `${hours} 时`;
}

/** The configuration the batch ran against, as one compact line. */
export function configSummary(batch: CodexProbeBatch): string {
  const profile = batch.config.profileName ?? "未关联档案";
  const model = batch.config.profileModel ?? "模型未知";
  const effort = batch.config.reasoningEffort
    ? `推理 ${batch.config.reasoningEffort}`
    : "推理自动";
  const connection = batch.config.connectionIdentity ?? "连接未知";
  return `${profile} · ${model} · ${effort} · ${connection}`;
}

export interface ProbeRunSummary {
  passedCount: number;
  judgedCount: number;
  recordedRuns: number;
  runCount: number;
  totalTokens: number | null;
  totalDurationMs: number | null;
  averageReasoning: number | null;
}

/** Aggregates that keep unknown usage unknown — a missing total is never
 * counted as zero. */
export function summarizeRuns(runs: CodexProbeRun[]): ProbeRunSummary {
  const finished = runs.filter((run) => run.status !== "running");
  const reasoningRuns = finished.filter((run) => run.reasoningTokens !== null);
  const totals = finished.flatMap((run) => run.totalTokens === null ? [] : [run.totalTokens]);
  const durations = finished.flatMap((run) => run.durationMs === null ? [] : [run.durationMs]);
  return {
    passedCount: finished.filter((run) => run.status === "passed").length,
    judgedCount: finished.filter((run) => run.status === "passed" || run.status === "failed").length,
    recordedRuns: totals.length,
    runCount: finished.length,
    totalTokens: totals.length ? totals.reduce((total, tokens) => total + tokens, 0) : null,
    totalDurationMs: durations.length
      ? durations.reduce((total, duration) => total + duration, 0)
      : null,
    averageReasoning: reasoningRuns.length
      ? Math.round(
          reasoningRuns.reduce((total, run) => total + (run.reasoningTokens ?? 0), 0) /
            reasoningRuns.length,
        )
      : null,
  };
}

const RUN_COLUMNS: Array<TableColumn<CodexProbeRun>> = [
  {
    key: "seq",
    header: "序号",
    render: (run) => `${run.seq}`,
  },
  {
    key: "result",
    header: "判分",
    cellClassName: "asb-codex-probe-detail",
    render: (run) => run.executionError ? (
      <span className={RUN_STATUS_CLASS[run.status]}>
        {RUN_STATUS_LABEL[run.status]} · {run.executionError}
      </span>
    ) : (
      <span className={RUN_STATUS_CLASS[run.status]}>{RUN_STATUS_LABEL[run.status]}</span>
    ),
  },
  {
    key: "answer",
    header: "最终回答",
    render: (run) => run.finalAnswer ?? "—",
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
    cellClassName: "asb-codex-probe-detail",
    render: (run) => <>
      {run.totalTokens === null ? "—" : formatTokenCount(run.totalTokens)}
      {run.usageError && <p className="asb-codex-probe-meta">用量不可用：{run.usageError}</p>}
    </>,
  },
  {
    key: "model",
    header: "模型",
    cellClassName: "asb-code",
    render: (run) => run.reportedModel ?? "—",
  },
  {
    key: "duration",
    header: "耗时",
    render: (run) => run.durationMs === null ? "—" : formatDuration(run.durationMs),
  },
];

/** The per-run detail table shared by the live panel and history detail. */
export function ProbeRunsTable({ batch }: { batch: CodexProbeBatch }) {
  return (
    <div className="asb-codex-probe-table-wrap">
      <Table columns={RUN_COLUMNS} rows={batch.runs} rowKey={(run) => `probe-run-${batch.batchId}-${run.seq}`}
        ariaLabel="降智雷达每次运行明细" className="asb-codex-probe-table" />
    </div>
  );
}

/** The summary card band shared by the live panel and history detail. */
export function probeSummaryCards(batch: CodexProbeBatch) {
  const summary = summarizeRuns(batch.runs);
  return [
    { label: "通过", value: `${summary.passedCount}/${summary.judgedCount}` },
    {
      label: summary.recordedRuns === summary.runCount && summary.runCount > 0
        ? "实测消耗" : "已记录消耗",
      value: summary.totalTokens === null ? "—" : formatCompactTokenCount(summary.totalTokens),
      unit: TOKEN_UNIT,
    },
    {
      label: "平均 reasoning",
      value: summary.averageReasoning === null ? "—" : formatCompactTokenCount(summary.averageReasoning),
      unit: TOKEN_UNIT,
    },
    { label: "总耗时", value: summary.totalDurationMs === null ? "—" : formatDuration(summary.totalDurationMs) },
  ];
}
