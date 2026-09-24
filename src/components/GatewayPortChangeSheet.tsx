import { errorText, localizedMessageText, uiMessage } from "../i18n/errors";
import type { LocalizedMessage } from "../api/client";
import { useEffect, useRef, useState } from "react";
import {
  cancelGatewayPortChange,
  commitGatewayPortChange,
  prepareGatewayPortChange,
  type AppKind,
  type GatewayPortChangePlan,
} from "../api/client";
import { useI18n } from "../i18n";
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
  warnings?: LocalizedMessage[];
  error?: unknown;
}

interface Props {
  state: PortSheetState;
  configuredPort: number;
  onState: (state: PortSheetState | null) => void;
  onRefreshed: () => void;
}

/** Input, preview, and confirmation for one backend-held port reservation. */
export function GatewayPortChangeSheet({ state, configuredPort, onState, onRefreshed }: Props) {
  const { t } = useI18n();
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
      onState({ ...state, error: uiMessage("gateway.errorPortRange", { min: MIN_PORT, max: MAX_PORT }) });
      return;
    }
    if (port === configuredPort) {
      onState({ ...state, error: uiMessage("gateway.errorSamePort") });
      return;
    }
    setBusy(true);
    try {
      const plan = await prepareGatewayPortChange(port);
      onState({ stage: "preview", port: state.port, plan, error: null });
    } catch (cause) {
      onState({ ...state, error: cause });
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
      onState({ stage: "input", port: state.port, error: cause });
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
      <div className="asb-dialog is-narrow" role="dialog" aria-modal="true" aria-label={t("gateway.sheetTitle")}>
        <header className="asb-dialog-heading">
          <h2 className="asb-dialog-title">{t("gateway.sheetTitle")}</h2>
        </header>
        <div className="asb-dialog-body">
          {state.stage === "input" && (
            <>
              <label className="asb-field is-narrow">
                <span>{t("gateway.newPortLabel", { port: configuredPort })}</span>
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
                {t("gateway.portRangeNote", { min: MIN_PORT, max: MAX_PORT })}
              </p>
            </>
          )}
          {state.stage === "preview" && state.plan && <Preview plan={state.plan} />}
          {state.stage === "done" && (
            <>
              <p className="asb-scope-note" role="status">
                {t("gateway.doneCopy", { port: state.resultToPort! })}
              </p>
              {state.warnings?.map((warning, index) => (
                <p key={`${index}:${warning.key}`} className="asb-warn-text" role="status">{localizedMessageText(warning, t)}</p>
              ))}
            </>
          )}
          {state.error != null && (
            <p className="asb-scope-note asb-fail-text" role="alert">
              {errorText(state.error, t)}
            </p>
          )}
        </div>
        <div className="asb-dialog-footer">
          {state.stage === "input" && (
            <>
              <Button variant="secondary" onClick={cancel}>{t("gateway.cancel")}</Button>
              <Button variant="primary" disabled={busy} onClick={() => void prepare()}>
                {busy ? t("gateway.validating") : t("gateway.nextPreview")}
              </Button>
            </>
          )}
          {state.stage === "preview" && (
            <>
              <Button variant="secondary" onClick={cancel}>{t("gateway.cancel")}</Button>
              <Button variant="primary" disabled={busy} onClick={() => void commit()}>
                {busy ? t("gateway.applying") : t("gateway.confirmApply")}
              </Button>
            </>
          )}
          {state.stage === "done" && <Button variant="secondary" onClick={cancel}>{t("gateway.close")}</Button>}
        </div>
      </div>
    </div>
  );
}

function Preview({ plan }: { plan: GatewayPortChangePlan }) {
  const { t } = useI18n();
  return (
    <ul className="asb-dialog-details">
      <li>{t("gateway.previewPort", { from: plan.fromPort, to: plan.toPort })}</li>
      {plan.clients.length === 0 ? (
        <li>{t("gateway.previewNoClients")}</li>
      ) : plan.clients.map((client) => (
        <li key={`${client.app}:${client.profileId}`}>
          {t("gateway.previewClientAddress", { name: client.profileName, app: APP_LABELS[client.app] })}<br />
          <span className="asb-num">{client.currentBaseUrl}</span><br />
          → <span className="asb-num">{client.newBaseUrl}</span>
        </li>
      ))}
      <li>{t("gateway.previewUnchanged")}</li>
    </ul>
  );
}
