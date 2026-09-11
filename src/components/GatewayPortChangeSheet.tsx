import { useEffect, useRef, useState } from "react";
import {
  cancelGatewayPortChange,
  commitGatewayPortChange,
  prepareGatewayPortChange,
  type AppKind,
  type GatewayPortChangePlan,
} from "../api/client";
import { Button } from "./Button";
import { Input } from "./Input";

const MIN_PORT = 1024;
const MAX_PORT = 65535;
const APP_LABELS: Record<AppKind, string> = { codex: "Codex", claude: "Claude Code" };

export interface PortSheetState {
  stage: "input" | "preview" | "done";
  port?: string;
  plan?: GatewayPortChangePlan;
  resultToPort?: number;
  warnings?: string[];
  error?: string | null;
}

interface Props {
  state: PortSheetState;
  configuredPort: number;
  onState: (state: PortSheetState | null) => void;
  onRefreshed: () => void;
}

/** Input, preview, and confirmation for one backend-held port reservation. */
export function GatewayPortChangeSheet({ state, configuredPort, onState, onRefreshed }: Props) {
  const [busy, setBusy] = useState(false);
  const completed = useRef(new Set<string>());
  const preparationId = state.plan?.preparationId;

  useEffect(
    () => () => {
      if (preparationId && !completed.current.has(preparationId)) {
        void cancelGatewayPortChange(preparationId);
      }
    },
    [preparationId],
  );

  const prepare = async () => {
    if (busy) return;
    const port = Number(state.port ?? "");
    if (!Number.isInteger(port) || port < MIN_PORT || port > MAX_PORT) {
      onState({ ...state, error: `监听端口必须是 ${MIN_PORT}–${MAX_PORT} 之间的整数` });
      return;
    }
    if (port === configuredPort) {
      onState({ ...state, error: "新端口与当前监听端口相同" });
      return;
    }
    setBusy(true);
    try {
      const plan = await prepareGatewayPortChange(port);
      onState({ stage: "preview", port: state.port, plan, error: null });
    } catch (cause) {
      onState({ ...state, error: messageFor(cause) });
    } finally {
      setBusy(false);
    }
  };

  const commit = async () => {
    if (!state.plan) return;
    setBusy(true);
    try {
      const result = await commitGatewayPortChange(state.plan.preparationId, true);
      completed.current.add(state.plan.preparationId);
      onRefreshed();
      onState({
        stage: "done",
        resultToPort: result.toPort,
        warnings: result.warnings,
        error: null,
      });
    } catch (cause) {
      // A commit consumes the one-shot server preparation even on failure.
      // Return to input instead of offering a misleading retry for that id.
      onState({ stage: "input", port: state.port, error: messageFor(cause) });
    } finally {
      setBusy(false);
    }
  };

  const cancel = () => {
    if (preparationId && !completed.current.has(preparationId)) {
      completed.current.add(preparationId);
      void cancelGatewayPortChange(preparationId);
    }
    onState(null);
  };

  return (
    <div
      className="asb-dialog-backdrop is-inline"
      onClick={(event) => event.target === event.currentTarget && cancel()}
    >
      <div className="asb-dialog is-narrow" role="dialog" aria-modal="true" aria-label="修改监听端口">
        <header className="asb-dialog-heading">
          <h2 className="asb-dialog-title">修改监听端口</h2>
        </header>
        <div className="asb-dialog-body">
          {state.stage === "input" && (
            <>
              <label className="asb-field">
                <span>新监听端口（当前 {configuredPort}）</span>
                <Input
                  type="number"
                  min={MIN_PORT}
                  max={MAX_PORT}
                  step={1}
                  value={state.port ?? ""}
                  autoFocus
                  onChange={(event) => onState({ ...state, port: event.target.value, error: null })}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void prepare();
                    }
                  }}
                />
              </label>
              <p className="asb-scope-note">
                允许 {MIN_PORT}–{MAX_PORT} 之间的整数；冲突或被系统拒绝时不会改动任何配置。
              </p>
            </>
          )}
          {state.stage === "preview" && state.plan && <Preview plan={state.plan} />}
          {state.stage === "done" && (
            <>
              <p className="asb-scope-note" role="status">
                监听端口已改为 {state.resultToPort}。配置文件已更新，但正在运行的客户端不会自动重新读取；
                请重新启动相关客户端或会话，使新地址生效。
              </p>
              {state.warnings?.map((warning) => (
                <p key={warning} className="asb-warn-text" role="status">{warning}</p>
              ))}
            </>
          )}
          {state.error && (
            <p className="asb-scope-note asb-fail-text" role="alert">
              {state.error}
            </p>
          )}
        </div>
        <div className="asb-dialog-footer">
          {state.stage === "input" && (
            <>
              <Button variant="secondary" onClick={cancel}>取消</Button>
              <Button variant="primary" disabled={busy} onClick={() => void prepare()}>
                {busy ? "正在校验…" : "下一步：预览变更"}
              </Button>
            </>
          )}
          {state.stage === "preview" && (
            <>
              <Button variant="secondary" onClick={cancel}>取消</Button>
              <Button variant="primary" disabled={busy} onClick={() => void commit()}>
                {busy ? "正在应用…" : "确认修改并应用"}
              </Button>
            </>
          )}
          {state.stage === "done" && <Button variant="secondary" onClick={cancel}>关闭</Button>}
        </div>
      </div>
    </div>
  );
}

function Preview({ plan }: { plan: GatewayPortChangePlan }) {
  return (
    <ul className="asb-dialog-details">
      <li>监听端口：{plan.fromPort} → {plan.toPort}</li>
      {plan.clients.length === 0 ? (
        <li>没有客户端正在使用本网关，仅更新网关监听端口。</li>
      ) : plan.clients.map((client) => (
        <li key={`${client.app}:${client.profileId}`}>
          {client.profileName}（{APP_LABELS[client.app]}）服务地址：<br />
          <span className="asb-num">{client.currentBaseUrl}</span><br />
          → <span className="asb-num">{client.newBaseUrl}</span>
        </li>
      ))}
      <li>上游地址、API 密钥与供应商参数保持不变；本机能力令牌不轮换。</li>
    </ul>
  );
}

function messageFor(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (
    typeof cause === "object"
    && cause !== null
    && "message" in cause
    && typeof cause.message === "string"
  ) {
    return cause.message;
  }
  return String(cause);
}
