import { useEffect, useId, useState } from "react";
import { CUSTOM_PROBE_QUESTION_ID, type ModelUsageRange } from "../api/client";
import { CodexOfficialResetPanel } from "../components/CodexOfficialResetPanel";
import { CodexProbePanel } from "../components/CodexProbePanel";
import { CodexResetPanel } from "../components/CodexResetPanel";
import { Button } from "../components/Button";
import { RadioOption } from "../components/RadioOption";
import { Select } from "../components/Select";
import { Tabs } from "../components/Tabs";
import { Time } from "../components/Time";
import { WorkspaceHeader } from "../components/WorkspaceHeader";
import { useCodexOfficialReset, useCodexResetSignal } from "../components/quota-reads";
import { useCodexProbe } from "../components/use-codex-probe";
import { UsagePage } from "../pages/UsagePage";
import { useModelUsageReport } from "../pages/use-model-usage-report";
import { USAGE_SECTIONS, type UsageSection } from "./navigation";

const RANGE_OPTIONS: ReadonlyArray<{ value: ModelUsageRange; label: string }> = [
  { value: "today", label: "今日" },
  { value: "last7Days", label: "近 7 天" },
  { value: "last30Days", label: "近 30 天" },
  { value: "all", label: "全部" },
];

const PROBE_RUN_COUNTS = [3, 5] as const;

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
  const [radarOpened, setRadarOpened] = useState(section === "radar");
  const [range, setRange] = useState<ModelUsageRange>("last7Days");
  useEffect(() => { if (section === "quota") setQuotaOpened(true); }, [section]);
  useEffect(() => { if (section === "radar") setRadarOpened(true); }, [section]);
  const consumptionActive = active && section === "consumption";
  const usage = useModelUsageReport(consumptionActive, range);
  const report = usage.read?.report ?? null;
  const radarActive = active && section === "radar";
  const probe = useCodexProbe(radarActive);
  const [questionId, setQuestionId] = useState<string | null>(null);
  const [runCount, setRunCount] = useState<number>(PROBE_RUN_COUNTS[0]);
  const [customQuestion, setCustomQuestion] = useState("");
  const [customAnswer, setCustomAnswer] = useState("");
  useEffect(() => {
    if (questionId === null && probe.questions && probe.questions.length > 0) {
      setQuestionId(probe.questions[0].id);
    }
  }, [questionId, probe.questions]);
  const customReady =
    questionId !== CUSTOM_PROBE_QUESTION_ID ||
    (customQuestion.trim().length > 0 && /^\d+$/.test(customAnswer.trim()));
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
        title="用量监控"
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
        ) : section === "radar" ? (
          <div className="asb-model-usage-controls">
            <Select
              value={questionId}
              options={[
                ...(probe.questions ?? []).map((question) => ({
                  value: question.id,
                  label: question.label,
                })),
                { value: CUSTOM_PROBE_QUESTION_ID, label: "自定义题" },
              ]}
              placeholder={probe.questions === null ? "题目加载中…" : "选择题目"}
              ariaLabel="探针题目"
              disabled={!radarActive || probe.running}
              onChange={setQuestionId}
            />
            <div className="asb-segments" role="radiogroup" aria-label="检测次数">
              {PROBE_RUN_COUNTS.map((count) => (
                <RadioOption
                  key={count}
                  name="codex-probe-run-count"
                  checked={runCount === count}
                  disabled={!radarActive || probe.running}
                  label={`${count} 次`}
                  onChange={() => setRunCount(count)}
                />
              ))}
            </div>
            {probe.running ? (
              <Button variant="secondary" onClick={() => void probe.cancel()}>
                取消检测
              </Button>
            ) : (
              <Button
                variant="primary"
                disabled={!radarActive || probe.starting || questionId === null || !customReady}
                onClick={() =>
                  void probe.start({
                    runCount,
                    questionId: questionId ?? "",
                    customQuestion:
                      questionId === CUSTOM_PROBE_QUESTION_ID ? customQuestion.trim() : undefined,
                    customAnswer:
                      questionId === CUSTOM_PROBE_QUESTION_ID ? customAnswer.trim() : undefined,
                  })
                }
              >
                {probe.starting ? "正在启动…" : "开始检测"}
              </Button>
            )}
          </div>
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
        ) : section === "radar" ? (
          <p className="asb-header-status" role="status">
            本机实测当前激活的 Codex 配置 · 每次检测都是真实调用并消耗额度
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
      <div id={`${id}-radar-panel`} role="tabpanel" aria-labelledby={`${id}-radar-tab`} hidden={section !== "radar"}>
        {radarOpened && (
          <CodexProbePanel
            probe={probe}
            form={{
              questionId,
              customQuestion,
              customAnswer,
              onCustomQuestionChange: setCustomQuestion,
              onCustomAnswerChange: setCustomAnswer,
            }}
          />
        )}
      </div>
    </section>
  );
}
