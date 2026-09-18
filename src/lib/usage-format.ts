import type { UsageReading, UsageSummary } from "../api/client";

const exactValueFormatter = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 });
const compactValueFormatter = new Intl.NumberFormat("zh-CN", {
  notation: "compact",
  maximumFractionDigits: 1,
});

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
  return normalized ? `${value} ${normalized}` : value;
}

export function usageProgress(reading: UsageReading): number | null {
  if (reading.total === null || !Number.isFinite(reading.total) || reading.total <= 0) return null;
  const used = reading.used ?? (reading.remaining === null ? null : reading.total - reading.remaining);
  if (used === null || !Number.isFinite(used)) return null;
  return (used / reading.total) * 100;
}

function compactUsageValue(value: number, unit: string | null): string {
  return appendUnit(compactValueFormatter.format(value), unit);
}

function usageReadingValues(
  reading: UsageReading,
  formatValue: (value: number, unit: string | null) => string,
): string[] {
  const values: string[] = [];
  if (reading.remaining !== null) values.push(`余额 ${formatValue(reading.remaining, reading.unit)}`);
  if (reading.used !== null) values.push(`已用 ${formatValue(reading.used, reading.unit)}`);
  if (reading.total !== null) values.push(`总量 ${formatValue(reading.total, reading.unit)}`);
  return values;
}

/** The primary row reflects non-null fields returned by the first script
 * reading. It does not derive a percentage or invent a missing value. */
export function formatUsageHighlight(summary: UsageSummary): string {
  const reading = summary.readings[0];
  if (!reading) return "暂无额度读数";

  const name = reading.planName?.trim();
  const values = usageReadingValues(reading, compactUsageValue);
  return [...(name ? [name] : []), ...values].join(" · ") || "暂无额度读数";
}

/** Tray balance line: only the first reading's balance is displayed, falling
 * back to the next reported value; it never derives or invents a number and
 * adds no heading copy. */
export function formatUsageBalance(summary: UsageSummary): string {
  const reading = summary.readings[0];
  if (!reading) return "暂无额度读数";
  const labeled = (label: string, value: number) =>
    `${label} ${compactUsageValue(value, reading.unit)}`;
  if (reading.remaining !== null) return labeled("余额", reading.remaining);
  if (reading.used !== null) return labeled("已用", reading.used);
  if (reading.total !== null) return labeled("总量", reading.total);
  return "暂无额度读数";
}

/** Full row/tray text preserves every non-null field returned by the
 * query. It never derives a display value that the script did not return. */
export function formatUsageSummary(summary: UsageSummary): string {
  if (summary.readings.length === 0) return "暂无额度读数";
  return summary.readings.map((reading) => {
    const name = reading.planName?.trim();
    const values = usageReadingValues(reading, (value, unit) => formatUsageValue(value, unit));
    const text = values.join(" · ") || "暂无额度读数";
    return name ? `${name}：${text}` : text;
  }).join("；");
}
