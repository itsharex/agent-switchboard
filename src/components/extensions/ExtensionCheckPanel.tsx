import { useEffect, useState } from "react";
import {
  cancelMcpCheck,
  checkMcpConnection,
  getMcpCheck,
  type ExtensionTarget,
  type McpCheckResult,
} from "../../api/client";
import { Button } from "../Button";

const STAGES = ["连接", "initialize", "读取目录", "完成"];
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
        badge: "通过",
        tone: "asb-ok-text",
        text: `协议 ${result.protocolVersion ?? "未知"} · 工具 ${outcome.tools} · 资源 ${outcome.resources} · 提示词 ${outcome.prompts}`,
      };
    case "partial":
      return { badge: "部分成功", tone: "asb-warn-text", text: outcome.error };
    case "failed":
      return { badge: "失败", tone: "asb-fail-text", text: `${outcome.classification}：${outcome.error}` };
    case "cancelled":
      return { badge: "已取消", tone: "", text: "检测已被取消" };
    case "needsNativeConfirmation":
      return {
        badge: "需要客户端验证",
        tone: "asb-warn-text",
        text: "该服务需要交互式登录；请在原生 /mcp 中完成确认",
      };
  }
}

/** One bounded MCP connection probe: starts the backend check, polls until a
 * result lands, and renders the stage stepper while waiting. */
export function ExtensionCheckPanel({ definitionId, target, busy }: Props) {
  const [checkId, setCheckId] = useState<string | null>(null);
  const [result, setResult] = useState<McpCheckResult | null>(null);
  const [stage, setStage] = useState(0);
  const [failure, setFailure] = useState<string | null>(null);

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
          setFailure("检测超时；请稍后重试");
          setCheckId(null);
          return;
        }
        pollTimer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS);
      } catch (caught) {
        if (stopped) return;
        setFailure((caught as { message?: string }).message ?? "检测轮询失败");
        setCheckId(null);
      }
    };
    pollTimer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS);
    // Bounded stage estimate: the backend reports only the final outcome, so
    // the stepper advances on fixed intervals and caps before 完成.
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
      setFailure((caught as { message?: string }).message ?? "无法启动连接检测");
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
      setFailure((caught as { message?: string }).message ?? "取消检测失败");
    }
  };

  const finished = result !== null;
  const activeStage = finished ? STAGES.length - 1 : stage;

  return (
    <div className="asb-ext-section" aria-label="连接检测">
      <h3>连接检测</h3>
      <div className="asb-ext-actions">
        <Button variant="secondary" disabled={busy || !target || running} onClick={() => void start()}>
          连接检测
        </Button>
        {running && (
          <Button variant="secondary" onClick={() => void cancel()}>
            取消检测
          </Button>
        )}
      </div>
      {(running || finished) && (
        <ol className="asb-ext-stages" aria-label="检测阶段">
          {STAGES.map((label, index) => (
            <li
              key={label}
              className={[
                "asb-ext-stage",
                index < activeStage ? "is-done" : "",
                index === activeStage && running ? "is-active" : "",
                index === activeStage && finished ? "is-done" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              {label}
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
            · 耗时 {result.durationMs} ms{result.truncated ? " · 目录结果被截断" : ""}
          </span>
        </p>
      )}
    </div>
  );
}
