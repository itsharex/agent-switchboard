import { useMessageState } from "../i18n/use-message-state";
import { useEffect, useRef, useState } from "react";
import { probeEndpoint, type ProbeResult } from "../api/client";
import { useI18n } from "../i18n";
import { Time } from "./Time";

interface FeedbackProps {
  result: ProbeResult | null;
  error: string | null;
}

/** A changed address or unmounted test invalidates pending probe results. */
export function useEndpointProbe(url: string | null) {
  const [result, setResult] = useState<ProbeResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useMessageState();
  const requestVersion = useRef(0);

  useEffect(() => {
    requestVersion.current += 1;
    setResult(null);
    setError(null);
    setBusy(false);
    return () => { requestVersion.current += 1; };
  }, [url]);

  const run = async () => {
    if (!url || busy) return;
    const version = requestVersion.current;
    setBusy(true);
    setError(null);
    try {
      const nextResult = await probeEndpoint(url);
      if (requestVersion.current === version) setResult(nextResult);
    } catch (caught) {
      if (requestVersion.current === version) {
        setError(caught);
      }
    } finally {
      if (requestVersion.current === version) setBusy(false);
    }
  };

  return { result, busy, error, run };
}

/** One result presentation for editor and supplier-card probe controls. */
export function ProbeFeedback({ result, error }: FeedbackProps) {
  const { t } = useI18n();
  if (!result && !error) return null;

  return (
    <div className="asb-probe-feedback" aria-live="polite">
      {result && (
        <span
          className={`asb-kv-label ${
            result.grade === "ok"
              ? "asb-ok-text"
              : result.grade === "slow"
                ? "asb-warn-text"
                : "asb-fail-text"
          }`}
        >
          {result.grade === "unreachable"
            ? t("providers.probe.unreachable", { detail: result.error ?? t("providers.probe.networkFailed") })
            : t("providers.probe.result", {
              grade: result.grade === "ok" ? t("providers.probe.ok") : t("providers.probe.slow"),
              status: result.status ?? "?",
              latency: result.latencyMs ?? "?",
            })}
          {" · "}
          <Time iso={result.at} />
        </span>
      )}
      {error ? (
        <p className="asb-warn-text" role="alert">{error}</p>
      ) : (
        result && (
          <p className="asb-scope-note">
            {t("providers.probe.note")}
          </p>
        )
      )}
    </div>
  );
}
