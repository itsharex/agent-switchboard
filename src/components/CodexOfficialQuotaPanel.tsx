import { useEffect, useState } from "react";
import type { CodexOfficialQuota } from "../api/client";
import { Input } from "./Input";
import { QuotaWindowsTable } from "./QuotaWindowsTable";
import type { CachedQuery } from "./use-cached-query";

interface Props {
  id: string;
  quota: CachedQuery<CodexOfficialQuota>;
  profileName: string;
  /** Backend auto-refresh cadence in minutes; 0 disables automatic queries. */
  refreshIntervalMinutes: number;
  /** Persists a committed interval on the owning profile; false restores the
   * previous value. */
  onSaveInterval: (minutes: number) => Promise<boolean>;
}

function QuotaRefreshInterval({
  refreshIntervalMinutes,
  onSaveInterval,
}: Pick<Props, "refreshIntervalMinutes" | "onSaveInterval">) {
  const [intervalText, setIntervalText] = useState(() => String(refreshIntervalMinutes));
  const [savingInterval, setSavingInterval] = useState(false);
  useEffect(() => {
    setIntervalText(String(refreshIntervalMinutes));
  }, [refreshIntervalMinutes]);

  /** Commits the free-form interval; anything outside whole minutes within
   * 0–1440, or a rejected save, reverts to the persisted value. Zero saves as
   * the profile's absent interval, the single manual-only representation. */
  const commitInterval = async () => {
    const text = intervalText.trim();
    if (!/^\d+$/.test(text) || Number(text) > 1440) {
      setIntervalText(String(refreshIntervalMinutes));
      return;
    }
    const minutes = Number(text);
    if (minutes === refreshIntervalMinutes) return;
    setSavingInterval(true);
    try {
      if (!(await onSaveInterval(minutes))) {
        setIntervalText(String(refreshIntervalMinutes));
      }
    } finally {
      setSavingInterval(false);
    }
  };

  return (
    <label className="asb-field asb-usage-interval">
      <span>自动刷新间隔（分钟，0 为关闭）</span>
      <Input
        type="number"
        min={0}
        max={1440}
        step={1}
        aria-label="自动刷新间隔（分钟，0 为关闭）"
        value={intervalText}
        disabled={savingInterval}
        onChange={(event) => setIntervalText(event.target.value)}
        onBlur={() => void commitInterval()}
        onKeyDown={(event) => {
          if (event.key === "Enter") void commitInterval();
        }}
      />
    </label>
  );
}

/** Row summary and details consume the same read; only the row owns polling. */
export function CodexOfficialQuotaPanel({
  id,
  quota,
  profileName,
  refreshIntervalMinutes,
  onSaveInterval,
}: Props) {
  const { data: reading } = quota;
  const showsWindows = (reading?.windows.length ?? 0) > 0;
  return (
    <section id={id} className="asb-official-quota" aria-label={`${profileName} 官方订阅额度`}>
      <header className="asb-provider-usage-head">
        <div className="asb-provider-usage-title">
          <h3 className="asb-section-title">订阅额度</h3>
        </div>
      </header>
      <QuotaRefreshInterval refreshIntervalMinutes={refreshIntervalMinutes} onSaveInterval={onSaveInterval} />
      {showsWindows && (
        <QuotaWindowsTable
          windows={reading!.windows}
          ariaLabel={`${profileName} 官方订阅额度`}
        />
      )}
    </section>
  );
}
