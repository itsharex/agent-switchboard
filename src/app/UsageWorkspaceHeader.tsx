import type { ReactNode } from "react";
import { CUSTOM_PROBE_QUESTION_ID, type ModelUsageRange } from "../api/client";
import { Button } from "../components/Button";
import type { CodexProbeFormState } from "../components/CodexProbePanel";
import { RadioOption } from "../components/RadioOption";
import { Select } from "../components/Select";
import { Tabs } from "../components/Tabs";
import { Time } from "../components/Time";
import { WorkspaceHeader } from "../components/WorkspaceHeader";
import type { useCodexOfficialReset, useCodexResetSignal } from "../components/quota-reads";
import type { useCodexProbe } from "../components/use-codex-probe";
import type { useModelUsageReport } from "../pages/use-model-usage-report";
import { USAGE_SECTIONS, type UsageSection } from "./navigation";

const RANGES: ReadonlyArray<{ value: ModelUsageRange; label: string }> = [
  { value: "today", label: "今日" }, { value: "last7Days", label: "近 7 天" },
  { value: "last30Days", label: "近 30 天" }, { value: "all", label: "全部" },
];

interface RadarControlsProps {
  active: boolean;
  probe: ReturnType<typeof useCodexProbe>;
  form: CodexProbeFormState;
  onQuestionChange: (id: string | null) => void;
  runCount: number;
  onRunCountChange: (count: number) => void;
  onOpenHistory: () => void;
}

function RadarControls({ active, probe, form, onQuestionChange, runCount, onRunCountChange, onOpenHistory }: RadarControlsProps) {
  const custom = form.questionId === CUSTOM_PROBE_QUESTION_ID;
  const ready = !custom || (form.customQuestion.trim().length > 0 && /^\d+$/.test(form.customAnswer.trim()));
  const locked = !active || probe.running || probe.starting || !!probe.status?.persistPending;
  return (
    <div className="asb-model-usage-controls">
      <Select value={form.questionId} options={[
        ...(probe.questions ?? []).map((question) => ({ value: question.id, label: question.label })),
        { value: CUSTOM_PROBE_QUESTION_ID, label: "自定义题" },
      ]} placeholder={probe.questions === null ? "题目加载中…" : "选择题目"}
        ariaLabel="探针题目" disabled={locked} onChange={onQuestionChange} />
      <div className="asb-segments" role="radiogroup" aria-label="检测次数">
        {[...new Set([3, 5, runCount])].sort((a, b) => a - b).map((count) => <RadioOption key={count} name="codex-probe-run-count"
          checked={runCount === count} disabled={locked} label={`${count} 次`}
          onChange={() => onRunCountChange(count)} />)}
      </div>
      {probe.running ? (
        <Button variant="secondary" disabled={probe.cancelling || probe.starting} onClick={() => void probe.cancel()}>
          {probe.cancelling ? "正在取消…" : "取消检测"}
        </Button>
      ) : (
        <Button variant="primary" disabled={locked || form.questionId === null || !ready}
          onClick={() => {
            if (form.questionId === null) return;
            void probe.start({ runCount, questionId: form.questionId,
              customQuestion: custom ? form.customQuestion.trim() : undefined,
              customAnswer: custom ? form.customAnswer.trim() : undefined });
          }}>
          {probe.starting ? "正在启动…" : "开始检测"}
        </Button>
      )}
      <Button variant="secondary" onClick={onOpenHistory}>历史记录</Button>
    </div>
  );
}

interface HeaderProps {
  id: string;
  section: UsageSection;
  onSectionChange: (section: UsageSection) => void;
  consumption: {
    active: boolean;
    range: ModelUsageRange;
    onRangeChange: (range: ModelUsageRange) => void;
    usage: ReturnType<typeof useModelUsageReport>;
  };
  officialReset: ReturnType<typeof useCodexOfficialReset>;
  resetSignal: ReturnType<typeof useCodexResetSignal>;
  radar: Omit<RadarControlsProps, "active">;
}

function ConsumptionControls({ active, range, onRangeChange, usage }: HeaderProps["consumption"]) {
  return (
    <div className="asb-model-usage-controls">
      <div className="asb-segments" role="radiogroup" aria-label="模型消耗时间范围">
        {RANGES.map((option) => <RadioOption key={option.value} name="model-usage-range"
          checked={range === option.value} disabled={!active} label={option.label}
          onChange={() => onRangeChange(option.value)} />)}
      </div>
      <Button variant="secondary" disabled={usage.loading || !active} onClick={() => void usage.refresh()}>
        {usage.loading ? "刷新中" : "刷新"}
      </Button>
    </div>
  );
}

function freshnessLabel(freshness: "cached" | "live") {
  return freshness === "cached" ? "本地缓存" : "刚刚刷新";
}

export function UsageWorkspaceHeader({ id, section, onSectionChange, consumption,
  officialReset, resetSignal, radar }: HeaderProps) {
  const { usage } = consumption;
  const report = usage.read?.report;
  let actions: ReactNode;
  let status: ReactNode;
  if (section === "consumption") {
    actions = <ConsumptionControls {...consumption} />;
    status = report ? <>
      {usage.read?.freshness === "cached" ? "本地快照" : "本次汇总"}：<Time iso={report.generatedAt} />
      {usage.loading ? " · 正在更新" : null}
    </> : "尚无本地汇总";
  } else if (section === "quota") {
    actions = <>
      <Button variant="secondary" disabled={officialReset.loading} onClick={() => void officialReset.readStatus()}>
        {officialReset.loading ? "读取中…" : "刷新官方额度"}
      </Button>
      <Button variant="secondary" disabled={resetSignal.loading} onClick={() => void resetSignal.readStatus()}>
        {resetSignal.loading ? "读取中…" : "刷新重置信号"}
      </Button>
    </>;
    status = <>
      官方额度：{officialReset.quota ? freshnessLabel(officialReset.freshness) : "尚无读取记录"}
      {" · "}重置信号：{resetSignal.snapshot ? freshnessLabel(resetSignal.snapshot.freshness) : "尚无缓存"}
    </>;
  } else {
    actions = <RadarControls active={section === "radar"} {...radar} />;
    status = "本机实测当前激活的 Codex 配置 · 每次检测都是真实调用并消耗额度 · 结果保存在本机检测历史";
  }
  return <WorkspaceHeader title="用量监控" primary={
    <Tabs value={section} onChange={onSectionChange} scope={id} label="用量分类"
      tabs={USAGE_SECTIONS.map((tab) => ({ ...tab, controls: `${id}-${tab.value}-panel` }))} />
  } primaryActions={actions} secondary={<p className="asb-header-status" role="status">{status}</p>} />;
}
