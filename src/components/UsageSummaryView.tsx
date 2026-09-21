import { RefreshCw } from "lucide-react";
import type { UsageReading } from "../api/client";
import { formatUsageValue, usagePrimary, usageTone, usageWindowName } from "../lib/usage-format";
import { Button } from "./Button";
import { Time } from "./Time";
import { Tooltip } from "./Tooltip";

function ReadingSummary({ reading }: { reading: UsageReading }) {
  const primary = usagePrimary(reading);
  return (
    <div className="asb-usage-reading" data-tone={usageTone(reading)}>
      {reading.planName && <span className="asb-usage-plan">{usageWindowName(reading.planName)}</span>}
      {primary && <span className="asb-usage-primary">
        <span>{primary.label}</span><strong>{formatUsageValue(primary.value, reading.unit)}</strong>
      </span>}
      {reading.isValid !== false && reading.remaining !== null && reading.remaining <= 0 && <span className="asb-usage-invalid">已耗尽</span>}
      {reading.isValid === false && <span className="asb-usage-invalid">{reading.invalidMessage || "已失效"}</span>}
      {reading.unit !== "%" && <span className="asb-usage-secondary">
        {reading.used !== null && primary?.label !== "已用" && <span>已用 {formatUsageValue(reading.used, reading.unit)}</span>}
        {reading.total !== null && primary?.label !== "总量" && <span>总量 {formatUsageValue(reading.total, reading.unit)}</span>}
      </span>}
      {reading.resetsAt && <span className="asb-usage-reset">重置：<Time iso={reading.resetsAt} mode="reset" /></span>}
      {reading.extra && <span className="asb-usage-note">{reading.extra}</span>}
    </div>
  );
}

interface Props {
  name: string;
  readings: UsageReading[];
  at: string | null;
  querying: boolean;
  error: string | null;
  stale?: boolean;
  empty: string;
  run: () => Promise<void>;
}

/** Complete decision facts stay visible; detailed tables are a separate disclosure. */
export function UsageSummaryView({ name, readings, at, querying, error, stale, empty, run }: Props) {
  const hasReadings = readings.length > 0;
  const previous = hasReadings && (stale || Boolean(error));
  return (
    <section className="asb-usage-summary" aria-label={`${name} 用量摘要`} aria-busy={querying}>
      <div className="asb-usage-readings">
        {hasReadings ? readings.map((reading, index) => <ReadingSummary key={index} reading={reading} />)
          : <span role="status">{error ? "用量读取失败" : querying ? "正在读取用量" : empty}</span>}
      </div>
      <div className="asb-usage-freshness">
        {previous && <span className="asb-usage-stale">上次读数{error ? " · 更新失败" : ""}</span>}
        {at && <span>更新于 <Time iso={at} mode="relative" /></span>}
        <Tooltip label={error ? `重试 ${name} 用量查询` : `刷新 ${name} 用量`}>
          <Button variant="icon" disabled={querying} aria-label={error ? `重试 ${name} 用量查询` : `刷新 ${name} 用量`}
            onClick={() => void run()}><RefreshCw size={14} /></Button>
        </Tooltip>
        {querying && <span role="status">更新中</span>}
      </div>
      {error && <p className="asb-usage-error" role="alert">{error}</p>}
    </section>
  );
}
