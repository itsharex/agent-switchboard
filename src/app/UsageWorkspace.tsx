import { useEffect, useId, useState } from "react";
import type { ModelUsageRange } from "../api/client";
import { CodexOfficialResetPanel } from "../components/CodexOfficialResetPanel";
import { CodexResetPanel } from "../components/CodexResetPanel";
import { Button } from "../components/Button";
import { RadioOption } from "../components/RadioOption";
import { Tabs } from "../components/Tabs";
import { Time } from "../components/Time";
import { WorkspaceHeader } from "../components/WorkspaceHeader";
import { useCodexOfficialReset, useCodexResetSignal } from "../components/quota-reads";
import { UsagePage } from "../pages/UsagePage";
import { useModelUsageReport } from "../pages/use-model-usage-report";
import { USAGE_SECTIONS, type UsageSection } from "./navigation";

const RANGE_OPTIONS: ReadonlyArray<{ value: ModelUsageRange; label: string }> = [
  { value: "today", label: "今日" },
  { value: "last7Days", label: "近 7 天" },
  { value: "last30Days", label: "近 30 天" },
  { value: "all", label: "全部" },
];

function freshnessLabel(freshness: "cached" | "live"): string {
  return freshness === "cached" ? "本地缓存" : "刚刚刷新";
}

export function UsageWorkspace({ active, section, onSectionChange }: {
  active: boolean;
  section: UsageSection;
  onSectionChange: (section: UsageSection) => void;
}) {
  const id = useId();
  const [quotaOpened, setQuotaOpened] = useState(section === "quota");
  const [range, setRange] = useState<ModelUsageRange>("last7Days");
  useEffect(() => { if (section === "quota") setQuotaOpened(true); }, [section]);
  const consumptionActive = active && section === "consumption";
  const usage = useModelUsageReport(consumptionActive, range);
  const report = usage.read?.report ?? null;
  const changeRange = (nextRange: ModelUsageRange) => {
    if (nextRange === range) return;
    setRange(nextRange);
  };
  const quotaReadsVisible = quotaOpened || section === "quota";
  const officialReset = useCodexOfficialReset(quotaReadsVisible);
  const resetSignal = useCodexResetSignal(quotaReadsVisible);
  return (
    <section className="asb-page-stack" aria-label="用量工作区">
      <WorkspaceHeader
        title="用量"
        primary={
          <Tabs value={section} onChange={onSectionChange} scope={id} label="用量分类"
            tabs={USAGE_SECTIONS.map((tab) => ({ ...tab, controls: `${id}-${tab.value}-panel` }))} />
        }
        primaryActions={section === "consumption" ? (
          <div className="asb-model-usage-controls">
            <div className="asb-segments" role="radiogroup" aria-label="模型消耗时间范围">
              {RANGE_OPTIONS.map((option) => (
                <RadioOption
                  key={option.value}
                  name="model-usage-range"
                  checked={range === option.value}
                  disabled={!consumptionActive}
                  label={option.label}
                  onChange={() => changeRange(option.value)}
                />
              ))}
            </div>
            <Button
              variant="secondary"
              disabled={usage.loading || !consumptionActive}
              onClick={() => void usage.refresh()}
            >
              {usage.loading ? "刷新中" : "刷新"}
            </Button>
          </div>
        ) : section === "quota" ? (
          <>
            <Button
              variant="secondary"
              disabled={officialReset.loading}
              onClick={() => void officialReset.readStatus()}
            >
              {officialReset.loading ? "读取中…" : "刷新官方额度"}
            </Button>
            <Button
              variant="secondary"
              disabled={resetSignal.loading}
              onClick={() => void resetSignal.readStatus()}
            >
              {resetSignal.loading ? "读取中…" : "刷新重置信号"}
            </Button>
          </>
        ) : undefined}
        secondary={section === "consumption" ? (
          <p className="asb-header-status" role="status">
            {report ? (
              <>
                {usage.read?.freshness === "cached" ? "本地快照" : "本次汇总"}：<Time iso={report.generatedAt} />
                {usage.loading ? " · 正在更新" : null}
              </>
            ) : (
              "尚无本地汇总"
            )}
          </p>
        ) : section === "quota" ? (
          <p className="asb-header-status" role="status">
            官方额度：{officialReset.quota ? freshnessLabel(officialReset.freshness) : "尚无读取记录"}
            {" · "}重置信号：{resetSignal.snapshot ? freshnessLabel(resetSignal.snapshot.freshness) : "尚无缓存"}
          </p>
        ) : undefined}
      />
      <div id={`${id}-consumption-panel`} role="tabpanel" aria-labelledby={`${id}-consumption-tab`} hidden={section !== "consumption"}>
        <UsagePage usage={usage} />
      </div>
      <div id={`${id}-quota-panel`} role="tabpanel" aria-labelledby={`${id}-quota-tab`} hidden={section !== "quota"}>
        {quotaReadsVisible && <div className="asb-page-stack">
          <CodexOfficialResetPanel read={officialReset} />
          <CodexResetPanel read={resetSignal} />
        </div>}
      </div>
    </section>
  );
}
