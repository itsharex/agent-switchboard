import type { CodexOfficialQuota, UsageReading } from "../api/client";

const exactValueFormatter = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 });
const compactValueFormatter = new Intl.NumberFormat("zh-CN", {
  notation: "compact",
  maximumFractionDigits: 1,
});

/** Official windows are percentages; generic balances keep their source unit. */
export function officialQuotaReadings(quota: CodexOfficialQuota): UsageReading[] {
  return quota.windows.map((window) => ({
    planName: window.label,
    remaining: 100 - window.usedPercent,
    used: window.usedPercent,
    total: 100,
    unit: "%",
    ...(window.resetsAt ? { resetsAt: window.resetsAt } : {}),
  }));
}

/** Formats a generic usage value with the unit carried by its source contract.
 * This keeps table values audit-friendly and leaves token-specific compaction
 * to the local-model token display contract. */
export function formatUsageValue(value: number | null | undefined, unit?: string | null): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  return appendUnit(exactValueFormatter.format(value), unit);
}

/** Compact axis-scale value. The chart caption owns the unit label, so this
 * intentionally returns only the scaled number rather than duplicating it on
 * every tick. */
export function formatCompactUsageValue(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return compactValueFormatter.format(value);
}

function appendUnit(value: string, unit: string | null | undefined): string {
  const normalized = unit?.trim();
  return normalized === "%" ? `${value}%` : normalized ? `${value} ${normalized}` : value;
}

export function usageProgress(reading: UsageReading): number | null {
  if (reading.total === null || !Number.isFinite(reading.total) || reading.total <= 0) return null;
  const used = reading.used ?? (reading.remaining === null ? null : reading.total - reading.remaining);
  if (used === null || !Number.isFinite(used)) return null;
  return (used / reading.total) * 100;
}

/** Source-reported remaining wins; missing fields are never fabricated. */
export function usagePrimary(reading: UsageReading): { label: string; value: number } | null {
  if (reading.remaining !== null) return { label: reading.unit === "%" ? "剩余" : "余额", value: reading.remaining };
  if (reading.used !== null) return { label: "已用", value: reading.used };
  if (reading.total !== null) return { label: "总量", value: reading.total };
  return null;
}

export function usageTone(reading: UsageReading): "danger" | undefined {
  return reading.isValid === false || (reading.remaining !== null && reading.remaining <= 0)
    ? "danger" : undefined;
}

const windowNames: Record<string, string> = {
  five_hour: "5 小时", weekly_limit: "7 天", seven_day: "7 天", monthly: "每月",
  "5 小时窗口": "5 小时", "每周窗口": "7 天", "每月窗口": "每月",
};

export function usageWindowName(name: string): string {
  return windowNames[name] ?? name;
}

export function compactUsageName(name: string): string {
  return usageWindowName(name).replace(/ 小时$/, "h").replace(/ 天$/, "d")
    .replace(/ 分钟$/, "m").replace(/^每月$/, "月");
}

export function formatTrayReading(reading: UsageReading): string {
  if (reading.isValid === false) return "已失效";
  const primary = usagePrimary(reading);
  if (!primary) return "暂无读数";
  const value = `${primary.label} ${appendUnit(compactValueFormatter.format(primary.value), reading.unit)}`;
  return reading.remaining !== null && reading.remaining <= 0 ? `${value} · 已耗尽` : value;
}
