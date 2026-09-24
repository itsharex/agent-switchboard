import { useEffect, useState } from "react";
import { useI18n } from "../i18n";
import { countdownLabel, relativeLabel, timeLabel } from "../lib/time";

/** All timestamp views share local-time formatting. Relative views tick only while visible. */
export function Time({ iso, mode = "absolute" }: { iso: string; mode?: "absolute" | "relative" | "reset" }) {
  useI18n();
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (mode === "absolute") return;
    let timer: ReturnType<typeof setInterval> | undefined;
    const sync = () => {
      clearInterval(timer);
      if (document.hidden) return;
      setNow(Date.now());
      timer = setInterval(() => setNow(Date.now()), 30_000);
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => { clearInterval(timer); document.removeEventListener("visibilitychange", sync); };
  }, [mode]);
  const label = mode === "relative" ? relativeLabel(iso, now)
    : mode === "reset" ? countdownLabel(iso, now) : timeLabel(iso);
  return <time dateTime={iso} title={mode === "absolute" ? undefined : timeLabel(iso)}>{label}</time>;
}
