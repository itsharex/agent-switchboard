import { useEffect, useRef, useState } from "react";
import {
  cancelCodexProbe,
  getCodexProbe,
  listCodexProbeQuestions,
  startCodexProbe,
  type CodexProbeQuestion,
  type CodexProbeRequest,
  type CodexProbeStatus,
} from "../api/client";

const POLL_INTERVAL_MS = 1000;

function errorMessage(reason: unknown): string {
  return reason instanceof Error && reason.message ? reason.message : "未提供具体原因";
}

/** One degradation-probe batch: start and cancel come from the page header,
 * this hook owns the polling lifecycle, and the panel renders the latest
 * status. The finished status stays on screen until the next start replaces
 * it, mirroring the backend's single probe slot. */
export function useCodexProbe(enabled: boolean) {
  const [questions, setQuestions] = useState<CodexProbeQuestion[] | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [status, setStatus] = useState<CodexProbeStatus | null>(null);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const probeIdRef = useRef<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const aliveRef = useRef(true);

  function stopPolling() {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }

  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
      stopPolling();
    };
  }, []);

  useEffect(() => {
    if (!enabled || questions !== null) return undefined;
    let active = true;
    listCodexProbeQuestions()
      .then((catalog) => {
        if (active) {
          setQuestions(catalog);
          setCatalogError(null);
        }
      })
      .catch((reason) => {
        if (active) setCatalogError(errorMessage(reason));
      });
    return () => {
      active = false;
    };
  }, [enabled, questions]);

  function watch(probeId: string) {
    probeIdRef.current = probeId;
    stopPolling();
    const tick = async () => {
      if (probeIdRef.current !== probeId || !aliveRef.current) return;
      try {
        const next = await getCodexProbe(probeId);
        if (probeIdRef.current !== probeId || !aliveRef.current) return;
        if (next === null) {
          stopPolling();
          return;
        }
        setStatus(next);
        if (next.phase !== "running") stopPolling();
      } catch {
        // A transient read failure is not a probe failure; keep polling.
      }
    };
    void tick();
    timerRef.current = window.setInterval(() => void tick(), POLL_INTERVAL_MS);
  }

  const start = async (request: CodexProbeRequest) => {
    if (starting || status?.phase === "running") return;
    setStarting(true);
    setStartError(null);
    try {
      const started = await startCodexProbe(request);
      if (!aliveRef.current) return;
      watch(started.probeId);
    } catch (reason) {
      if (aliveRef.current) setStartError(errorMessage(reason));
    } finally {
      if (aliveRef.current) setStarting(false);
    }
  };

  const cancel = async () => {
    const probeId = probeIdRef.current;
    if (probeId === null) return;
    try {
      await cancelCodexProbe(probeId);
    } catch {
      // The next poll reflects the real outcome; nothing to surface here.
    }
  };

  return {
    questions,
    catalogError,
    status,
    starting,
    startError,
    running: status?.phase === "running",
    start,
    cancel,
  };
}
