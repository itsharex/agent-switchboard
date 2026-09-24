import { uiMessage } from "../../i18n/errors";
import { useMessageState } from "../../i18n/use-message-state";
import { useEffect, useState } from "react";
import {
  cancelMcpCheck,
  checkMcpConnection,
  getMcpCheck,
  type ExtensionTarget,
  type McpCheckResult,
} from "../../api/client";
import { useI18n } from "../../i18n";
import { tr } from "../../i18n/current";
import { Button } from "../Button";

const STAGES = [
  "extensions.check.stage.connect",
  "extensions.check.stage.initialize",
  "extensions.check.stage.tools",
  "extensions.check.stage.done",
] as const;
const POLL_INTERVAL_MS = 300;
const MAX_POLLS = 90;
const STAGE_ADVANCE_MS = 1600;

interface Props {
  definitionId: string;
  /** Target the probe runs against; absent until a target is chosen. */
  target: ExtensionTarget | null;
  busy: boolean;
}

function outcomeView(result: McpCheckResult): { badge: string; tone: string; text: string } {
  const outcome = result.outcome;
  switch (outcome.kind) {
    case "passed":
      return {
        badge: tr("extensions.check.passed"),
        tone: "asb-ok-text",
        text: tr("extensions.check.passedDetail", {
          protocol: result.protocolVersion ?? tr("extensions.check.unknown"),
          tools: outcome.tools,
          resources: outcome.resources,
          prompts: outcome.prompts,
        }),
      };
    case "partial":
      return { badge: tr("extensions.check.partial"), tone: "asb-warn-text", text: outcome.error };
    case "failed":
      return {
        badge: tr("extensions.check.failed"),
        tone: "asb-fail-text",
        text: tr("extensions.check.failedDetail", { classification: outcome.classification, message: outcome.error }),
      };
    case "cancelled":
      return { badge: tr("extensions.check.cancelled"), tone: "", text: tr("extensions.check.cancelledDetail") };
    case "needsNativeConfirmation":
      return {
        badge: tr("extensions.check.needsNativeConfirmation"),
        tone: "asb-warn-text",
        text: tr("extensions.check.nativeConfirmationDetail"),
      };
  }
}

/** One bounded MCP connection probe: starts the backend check, polls until a
 * result lands, and renders the stage stepper while waiting. */
export function ExtensionCheckPanel({ definitionId, target, busy }: Props) {
  const { t } = useI18n();
  const [checkId, setCheckId] = useState<string | null>(null);
  const [result, setResult] = useState<McpCheckResult | null>(null);
  const [stage, setStage] = useState(0);
  const [failure, setFailure] = useMessageState();

  useEffect(() => {
    setCheckId(null);
    setResult(null);
    setStage(0);
    setFailure(null);
  }, [definitionId, target]);

  useEffect(() => {
    if (!checkId) return;
    let stopped = false;
    let polls = 0;
    let pollTimer = 0;
    const tick = async () => {
      polls += 1;
      try {
        const found = await getMcpCheck(checkId);
        if (stopped) return;
        if (found !== null) {
          setResult(found);
          setCheckId(null);
          return;
        }
        if (polls >= MAX_POLLS) {
          setFailure(uiMessage("extensions.check.timeout"));
          setCheckId(null);
          return;
        }
        pollTimer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS);
      } catch (caught) {
        if (stopped) return;
        setFailure(caught);
        setCheckId(null);
      }
    };
    pollTimer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS);
    // Bounded stage estimate: the backend reports only the final outcome, so
    // the stepper advances on fixed intervals and caps before the last stage.
    const advance = window.setInterval(() => {
      setStage((current) => Math.min(current + 1, STAGES.length - 2));
    }, STAGE_ADVANCE_MS);
    return () => {
      stopped = true;
      window.clearTimeout(pollTimer);
      window.clearInterval(advance);
    };
  }, [checkId]);

  const running = checkId !== null;

  const start = async () => {
    if (!target || busy || running) return;
    setFailure(null);
    setResult(null);
    setStage(0);
    try {
      const started = await checkMcpConnection(definitionId, target, true);
      setCheckId(started.checkId);
    } catch (caught) {
      setFailure(caught);
    }
  };

  const cancel = async () => {
    const id = checkId;
    if (!id) return;
    setCheckId(null);
    setStage(0);
    try {
      await cancelMcpCheck(id);
    } catch (caught) {
      setFailure(caught);
    }
  };

  const finished = result !== null;
  const activeStage = finished ? STAGES.length - 1 : stage;

  return (
    <div className="asb-ext-section" aria-label={t("extensions.check.title")}>
      <div className="asb-ext-section-heading">
        <h3 className="asb-section-title">{t("extensions.check.title")}</h3>
        <div className="asb-ext-actions">
          <Button variant="secondary" disabled={busy || !target || running} onClick={() => void start()}>
            {t("extensions.check.run")}
          </Button>
          {running && (
            <Button variant="secondary" onClick={() => void cancel()}>
              {t("confirm.cancel")}
            </Button>
          )}
        </div>
      </div>
      {(running || finished) && (
        <ol className="asb-ext-stages" aria-label={t("extensions.check.stagesAria")}>
          {STAGES.map((stageKey, index) => (
            <li
              key={stageKey}
              className={[
                "asb-ext-stage",
                index < activeStage ? "is-done" : "",
                index === activeStage && running ? "is-active" : "",
                index === activeStage && finished ? "is-done" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              {t(stageKey)}
            </li>
          ))}
        </ol>
      )}
      {failure && (
        <p className="asb-warn-text" role="alert">
          {failure}
        </p>
      )}
      {result && (
        <p className="asb-ext-check-result">
          <span className="asb-pill-status">{outcomeView(result).badge}</span>{" "}
          <span className={outcomeView(result).tone}>{outcomeView(result).text}</span>
          <span className="asb-scope-note">
            {" "}
            {t("extensions.check.duration", { duration: result.durationMs })}
            {result.truncated ? t("extensions.check.truncated") : ""}
          </span>
        </p>
      )}
    </div>
  );
}
