import { useEffect, useRef, useState } from "react";
import { probeEndpoint, type ProbeResult } from "../api/client";
import { Time } from "./Time";

interface FeedbackProps {
  result: ProbeResult | null;
  error: string | null;
}

/** A changed address or unmounted test invalidates pending probe results. */
export function useEndpointProbe(url: string | null) {
  const [result, setResult] = useState<ProbeResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
        setError((caught as { message?: string }).message ?? "检测失败");
      }
    } finally {
      if (requestVersion.current === version) setBusy(false);
    }
  };

  return { result, busy, error, run };
}

/** One result presentation for editor and supplier-card probe controls. */
export function ProbeFeedback({ result, error }: FeedbackProps) {
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
            ? `无法连通 · ${result.error ?? "网络请求失败"}`
            : `${result.grade === "ok" ? "连通正常" : "连通但较慢"} · HTTP ${result.status ?? "?"} · ${result.latencyMs ?? "?"} 毫秒`}
          {" · "}
          <Time iso={result.at} />
        </span>
      )}
      {error ? (
        <p className="asb-warn-text" role="alert">{error}</p>
      ) : (
        result && (
          <p className="asb-scope-note">
            检测仅确认服务地址可达，不发送模型请求，也不验证密钥是否有效。
          </p>
        )
      )}
    </div>
  );
}
