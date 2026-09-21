import { useLayoutEffect, useRef, useState } from "react";
import type { TrayUsage as Usage, UsageReading } from "../api/client";
import { compactUsageName, formatTrayReading, officialQuotaReadings, usageTone } from "../lib/usage-format";

export function trayUsageContent(usage: Usage | null) {
  if (!usage) return { readings: [], stale: false, state: null };
  if (usage.kind === "script") {
    const readings = usage.reading.summary?.readings ?? [];
    return { readings, stale: readings.length > 0 && Boolean(usage.reading.error),
      state: readings.length === 0 && usage.reading.error ? "查询失败" : null };
  }
  const readings = officialQuotaReadings(usage.reading);
  const status = usage.reading.status;
  const state = status === "signInRequired" ? "待登录"
    : status === "reauthenticationRequired" ? "需重新登录"
    : status === "unavailable" ? "查询失败" : null;
  return { readings, stale: readings.length > 0 && (usage.reading.stale || Boolean(state)), state };
}

function Reading({ reading }: { reading: UsageReading }) {
  return <span className="tray-usage-reading" data-tone={usageTone(reading)}>
    {reading.planName && <span className="tray-usage-name">{compactUsageName(reading.planName)}</span>}
    <span className="tray-usage-value">{formatTrayReading(reading)}</span>
  </span>;
}

export function TrayUsage({ usage }: { usage: Usage | null }) {
  const { readings, stale, state } = trayUsageContent(usage);
  const host = useRef<HTMLSpanElement>(null);
  const measure = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(2);
  const total = readings.length;
  const signature = JSON.stringify({ readings, stale, state });
  useLayoutEffect(() => {
    const element = host.current;
    const ruler = measure.current;
    if (!element || !ruler || !element.parentElement) return;
    const resize = () => {
      const row = element.parentElement!;
      const available = row.clientWidth * 0.60;
      const children = Array.from(ruler.children) as HTMLElement[];
      const statusWidth = children.filter((child) => child.dataset.measure === "status")
        .reduce((sum, child) => sum + child.offsetWidth + 8, 0);
      const values = children.filter((child) => child.dataset.measure === "reading");
      let width = statusWidth;
      let count = 0;
      for (const child of values) {
        const more = count + 1 < total ? 40 : 0;
        if (width + child.offsetWidth + 8 + more > available) break;
        width += child.offsetWidth + 8;
        count += 1;
      }
      setVisible(count);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element.parentElement);
    observer.observe(ruler);
    return () => observer.disconnect();
  }, [signature, total]);
  if (readings.length === 0 && !state) return null;
  const states = <>{state && <span className="tray-usage-state">{state}</span>}
    {stale && <span className="tray-usage-state">上次读数</span>}</>;
  return <span ref={host} className="tray-provider-balance">
    {readings.slice(0, visible).map((reading, index) => <Reading key={index} reading={reading} />)}
    {readings.length > visible && <span className="tray-usage-more">+{readings.length - visible}</span>}
    {states}
    <span ref={measure} className="tray-usage-measure" aria-hidden="true">
      {readings.slice(0, 2).map((reading, index) => <span key={index} data-measure="reading"><Reading reading={reading} /></span>)}
      <span data-measure="status">{states}</span>
    </span>
  </span>;
}
