import { useLayoutEffect, useRef, useState } from "react";
import type { TrayUsage as Usage, UsageReading } from "../api/client";
import { formatTrayReading, officialQuotaReadings, usageTone } from "../lib/usage-format";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/messages";

interface TrayUsageState {
  readings: UsageReading[];
  stale: boolean;
  stateKey: MessageKey | null;
}

export function trayUsageContent(usage: Usage | null): TrayUsageState {
  if (!usage) return { readings: [], stale: false, stateKey: null };
  if (usage.kind === "script") {
    const readings = usage.reading.summary?.readings ?? [];
    return { readings, stale: readings.length > 0 && Boolean(usage.reading.error),
      stateKey: readings.length === 0 && usage.reading.error ? "tray.state.queryFailed" : null };
  }
  const readings = officialQuotaReadings(usage.reading, true);
  const status = usage.reading.status;
  const stateKey: MessageKey | null = status === "signInRequired" ? "tray.state.signInRequired"
    : status === "reauthenticationRequired" ? "tray.state.reauthRequired"
    : status === "unavailable" ? "tray.state.queryFailed" : null;
  return { readings, stale: readings.length > 0 && (usage.reading.stale || Boolean(stateKey)), stateKey };
}

function Reading({ reading }: { reading: UsageReading }) {
  return <span className="tray-usage-reading" data-tone={usageTone(reading)}>
    {reading.planName && <span className="tray-usage-name">{reading.planName}</span>}
    <span className="tray-usage-value">{formatTrayReading(reading)}</span>
  </span>;
}

export function TrayUsage({ usage }: { usage: Usage | null }) {
  const { t } = useI18n();
  const { readings, stale, stateKey } = trayUsageContent(usage);
  const host = useRef<HTMLSpanElement>(null);
  const measure = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(2);
  const total = readings.length;
  const signature = JSON.stringify({ readings, stale, stateKey });
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
  if (readings.length === 0 && !stateKey) return null;
  const states = <>{stateKey && <span className="tray-usage-state">{t(stateKey)}</span>}
    {stale && <span className="tray-usage-state">{t("tray.state.stale")}</span>}</>;
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
