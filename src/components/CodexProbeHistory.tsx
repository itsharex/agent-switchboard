import { useMemo, useState } from "react";
import type { CodexProbeHistoryItem } from "../api/client";
import { Button } from "./Button";
import { Checkbox } from "./Checkbox";
import { ConfirmSheet } from "./ConfirmSheet";
import { EditorFrame } from "./EditorFrame";
import { Pagination } from "./Pagination";
import { Select } from "./Select";
import { StatCards } from "./charts/stat-cards";
import {
  BATCH_STATUS_CLASS, BATCH_STATUS_LABEL, ProbeRunsTable, configSummary, probeSummaryCards,
  summarizeRuns,
} from "./probe-shared";
import { Table, type TableColumn } from "./Table";
import { Time } from "./Time";
import { ModuleHeader } from "./WorkspaceHeader";
import { formatCompactTokenCount, TOKEN_UNIT } from "../lib/token-format";
import type { useCodexProbe } from "./use-codex-probe";
import { useCodexProbeHistory } from "./use-codex-probe-history";

interface Props {
  active: boolean;
  probe: ReturnType<typeof useCodexProbe>;
  onClose: () => void;
  /** Fills the radar form with this question; the user starts it against
   * the current configuration. */
  onReuseQuestion: (question: { id: string; text: string; expectedAnswer: string }, runCount: number) => void;
  /** Notifies the radar panel that history changed (e.g. deletions). */
  onChanged: () => void;
}

const RANGE_OPTIONS = [
  { value: "all", label: "全部时间" },
  { value: "last7Days", label: "近 7 天" },
  { value: "last30Days", label: "近 30 天" },
];

const STATUS_OPTIONS = [
  { value: "all", label: "全部状态" },
  { value: "running", label: "进行中" },
  { value: "completed", label: "已完成" },
  { value: "cancelled", label: "已取消" },
  { value: "failed", label: "失败" },
  { value: "interrupted", label: "已中断" },
  { value: "config-changed", label: "配置已变化" },
];

function tokensCell(row: CodexProbeHistoryItem) {
  if (row.totalTokens === null) return row.runCount === 0 ? "—" : "未知";
  const coverage = row.recordedRuns < row.runCount
    ? `（${row.recordedRuns}/${row.runCount} 已记录）`
    : "";
  return `${formatCompactTokenCount(row.totalTokens)} ${TOKEN_UNIT}${coverage}`;
}

function historyColumns(history: ReturnType<typeof useCodexProbeHistory>): Array<TableColumn<CodexProbeHistoryItem>> {
  return [
    {
      key: "select",
      header: "选择",
      render: (row) => <Checkbox checked={history.selection.has(row.batchId)}
        disabled={row.status === "running" || history.deleting} label=""
        ariaLabel={`选择 ${row.startedAt} 的检测记录`}
        onChange={(checked) => history.toggleSelected(row.batchId, checked)} />,
    },
    {
      key: "time",
      header: "检测时间",
      render: (row) => <Time iso={row.startedAt} />,
    },
    {
      key: "config",
      header: "当时配置",
      cellClassName: "asb-codex-probe-detail",
      render: (row) => row.profileName ?? "未关联档案",
    },
    {
      key: "question",
      header: "题目",
      render: (row) => row.questionLabel,
    },
    {
      key: "status",
      header: "状态",
      render: (row) => <span className={BATCH_STATUS_CLASS[row.status]}>
        {BATCH_STATUS_LABEL[row.status]}
      </span>,
    },
    {
      key: "passed",
      header: "通过/已判定",
      render: (row) => `${row.passedCount}/${row.judgedCount}`,
    },
    {
      key: "tokens",
      header: "消耗",
      cellClassName: "asb-num",
      render: tokensCell,
    },
    {
      key: "view",
      header: "操作",
      render: (row) => <Button variant="secondary" onClick={() => history.openDetail(row.batchId)}>
        查看
      </Button>,
    },
  ];
}

function HistoryList({ history, onRequestDelete }: {
  history: ReturnType<typeof useCodexProbeHistory>;
  onRequestDelete: (batchIds: string[]) => void;
}) {
  const columns = historyColumns(history);
  return (
    <div className="asb-codex-probe-history">
      <div className="asb-model-usage-controls">
        <Select value={history.range} options={RANGE_OPTIONS} ariaLabel="检测时间范围"
          onChange={(value) => history.changeRange(value as typeof history.range)} />
        <Select value={history.profileFilter} options={[
          { value: "all", label: "全部档案" },
          { value: "unlinked", label: "未关联档案" },
          ...(history.profiles ?? []).map((profile) => ({
            value: profile.profileId,
            label: profile.profileName,
          })),
        ]} placeholder={history.profiles === null ? "档案加载中…" : "选择档案"}
          ariaLabel="档案筛选" onChange={history.changeProfile} />
        <Select value={history.statusFilter} options={STATUS_OPTIONS} ariaLabel="检测状态筛选"
          onChange={history.changeStatus} />
        <Button variant="secondary" disabled={history.loading} onClick={history.refresh}>刷新</Button>
        <Button variant="danger" disabled={history.selection.size === 0 || history.deleting}
          onClick={() => onRequestDelete([...history.selection])}>
          删除选中（{history.selection.size}）
        </Button>
      </div>
      {history.profilesError && <p className="asb-warn-text" role="alert">
        档案筛选加载失败：{history.profilesError}
      </p>}
      {history.listError && <p className="asb-warn-text" role="alert">检测历史读取失败：{history.listError}</p>}
      {history.deleteError && <p className="asb-warn-text" role="alert">删除失败：{history.deleteError}</p>}
      <p className="asb-codex-probe-meta asb-num" role="status">
        共 {history.total} 条检测记录{history.loading ? " · 读取中…" : ""}
      </p>
      {history.items.length > 0 && <>
        <Table columns={columns} rows={history.items} ariaLabel="检测历史列表"
          rowKey={(row) => row.batchId} />
        <Pagination total={history.total} page={history.page} pageSize={history.pageSize}
          onPageChange={history.setPage} label="检测历史分页" />
      </>}
      {history.items.length === 0 && !history.loading && (
        <div className="asb-empty-state">
          <p className="asb-section-title">
            {history.total === 0 && history.range === "all" && history.profileFilter === "all"
              && history.statusFilter === "all"
              ? "暂无检测历史。完成检测后，批次与逐次结果会保存在本机。"
              : "没有符合筛选条件的检测记录。"}
          </p>
          {history.total > 0 && <Button variant="secondary" onClick={history.clearFilters}>清除筛选</Button>}
        </div>
      )}
    </div>
  );
}

function HistoryDetail({ history, onReuseQuestion }: {
  history: ReturnType<typeof useCodexProbeHistory>;
  onReuseQuestion: Props["onReuseQuestion"];
}) {
  const batch = history.detail;
  const summary = batch ? summarizeRuns(batch.runs) : null;
  return (
    <div className="asb-codex-probe-history">
      <ModuleHeader title="检测详情" primaryActions={
        <>
          {batch?.status === "running" &&
            <Button variant="secondary" disabled={history.detailLoading}
              onClick={() => history.detailId && history.openDetail(history.detailId)}>
              刷新
            </Button>}
          <Button variant="secondary" onClick={history.closeDetail}>返回列表</Button>
        </>
      } />
      {history.detailLoading && <p className="asb-codex-probe-meta" role="status">正在读取检测详情…</p>}
      {history.detailError && <p className="asb-warn-text" role="alert">{history.detailError}</p>}
      {batch && <>
        <p className="asb-codex-probe-meta">
          状态：<span className={BATCH_STATUS_CLASS[batch.status]}>{BATCH_STATUS_LABEL[batch.status]}</span>
          {" · "}开始于 <Time iso={batch.startedAt} />
          {batch.finishedAt && <> · 结束于 <Time iso={batch.finishedAt} /></>}
        </p>
        {batch.statusError && <p className="asb-warn-text" role="alert">{batch.statusError}</p>}
        {batch.persistPending && <p className="asb-warn-text" role="alert">
          {batch.persistError ?? "该批次有部分结果未能保存。"}
        </p>}
        <p className="asb-codex-probe-meta">
          当时配置：{configSummary(batch)}{batch.cliVersion && ` · CLI ${batch.cliVersion}`}
        </p>
        <div className="asb-codex-probe-custom">
          <p className="asb-codex-probe-meta">
            题目（{batch.question.label}）· 判分规则 v{batch.gradingVersion} · 期望答案 {batch.question.expectedAnswer}
          </p>
          <p>{batch.question.text}</p>
        </div>
        {summary && summary.runCount > 0 && <>
          <div role="group" aria-label="检测结果汇总"><StatCards stats={probeSummaryCards(batch)} /></div>
          {summary.recordedRuns < summary.runCount && <p className="asb-codex-probe-note" role="status">
            仅 {summary.recordedRuns}/{summary.runCount} 次取得总消耗记录；缺失用量未知，汇总不代表完整消耗。
          </p>}
          <ProbeRunsTable batch={batch} />
        </>}
        {summary && summary.runCount === 0 && !history.detailLoading &&
          <p className="asb-codex-probe-meta">该批次没有已记录的运行。</p>}
        <div className="asb-model-usage-controls">
          <Button variant="primary" onClick={() => onReuseQuestion(batch.question, batch.plannedRuns)}>
            使用此题重新检测
          </Button>
        </div>
        <p className="asb-codex-probe-note">
          重新检测只填入题目与次数，点击开始后将以当前激活的 Codex 配置执行，并作为新批次记录。
        </p>
      </>}
    </div>
  );
}

/** The full-page probe history on the shared editor frame: filtered and
 * paged list, detail view, explicit deletions, and a live entry back to a
 * running batch. */
export function CodexProbeHistory({ active, probe, onClose, onReuseQuestion, onChanged }: Props) {
  const batch = probe.status;
  const revision = `${batch?.batchId}:${batch?.status}:${batch?.completedRuns}:${batch?.persistPending}`;
  const history = useCodexProbeHistory(active, revision);
  const [pendingDelete, setPendingDelete] = useState<string[] | null>(null);
  const pendingItems = useMemo(
    () => history.items.filter((item) => pendingDelete?.includes(item.batchId)),
    [history.items, pendingDelete],
  );
  const running = probe.running && probe.status;
  const confirmDelete = async () => {
    const batchIds = pendingDelete ?? [];
    setPendingDelete(null);
    const deleted = await history.deleteSelected(batchIds);
    if (deleted) {
      onChanged();
      if (history.detailId && batchIds.includes(history.detailId)) {
        history.closeDetail();
      }
    }
  };
  return (
    <EditorFrame
      title="检测历史"
      backLabel="返回降智雷达"
      onBack={onClose}
      primary={running ? (
        <span className="asb-header-status" role="status">
          检测进行中 · 已完成 {running.completedRuns}/{running.plannedRuns} 次
          {" "}
          <Button variant="secondary" onClick={onClose}>返回查看</Button>
        </span>
      ) : undefined}
    >
      {history.detailId === null
        ? <HistoryList history={history} onRequestDelete={setPendingDelete} />
        : <HistoryDetail history={history} onReuseQuestion={onReuseQuestion} />}
      {pendingDelete !== null && pendingItems.length > 0 && (
        <ConfirmSheet
          title="删除检测历史"
          confirmLabel="确认删除"
          destructive
          confirmDisabled={history.deleting}
          onConfirm={() => void confirmDelete()}
          onCancel={() => setPendingDelete(null)}
        >
          <ul className="asb-dialog-details">
            <li>数量 {pendingItems.length} 条检测批次</li>
            <li>时间范围 <Time iso={pendingItems.map((item) => item.startedAt).sort()[0]} />
              {" "}至 <Time iso={
                pendingItems.map((item) => item.startedAt).sort()[
                  pendingItems.length - 1
                ]
              } /></li>
            <li>仅删除降智雷达自身的记录；Codex 会话与「消耗统计」不受影响。</li>
            <li>删除后不可恢复。</li>
          </ul>
        </ConfirmSheet>
      )}
    </EditorFrame>
  );
}
