import { commandErrorText } from "../i18n/errors";
import { useId } from "react";
import type { CodexSubagentSettings, SettingValue } from "../api/client";
import type { CodexSubagentSettingsEditorState } from "../app/useCodexSubagentSettings";
import type { MessageKey, TFunction } from "../i18n";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { Input } from "./Input";
import { RadioOption } from "./RadioOption";

type SubagentField = keyof CodexSubagentSettings;

interface Props {
  editorState: CodexSubagentSettingsEditorState;
  busy: boolean;
  onChange: (field: SubagentField, value: SettingValue) => void;
  onRetryLoad: () => void;
}

const automatic: SettingValue = { mode: "automatic" };

function explicit(value: boolean | string | number): SettingValue {
  return { mode: "explicit", value };
}

function numericValue(value: SettingValue): string {
  return value.mode === "explicit" ? String(value.value) : "";
}

function actualValueLabel(value: SettingValue, t: TFunction): string {
  if (value.mode === "automatic") return t("codex.subagent.auto");
  if (typeof value.value === "boolean") return value.value ? t("codex.subagentPanel.on") : t("codex.subagentPanel.off");
  return String(value.value);
}

function draftIssue(settings: CodexSubagentSettings, t: TFunction): string | null {
  const concurrency = settings.maxConcurrentThreadsPerSession;
  if (concurrency.mode === "explicit" &&
    (typeof concurrency.value !== "number" || !Number.isSafeInteger(concurrency.value) || concurrency.value < 1)) {
    return t("codex.subagentPanel.concurrentInvalid");
  }
  for (const [labelKey, value] of [
    ["codex.subagentPanel.enabledLabel", settings.enabled],
    ["codex.subagentPanel.interruptLabel", settings.interruptMessage],
  ] as const) {
    if (value.mode === "explicit" && typeof value.value !== "boolean") {
      return t("codex.subagentPanel.booleanInvalid", { label: t(labelKey as MessageKey) });
    }
  }
  return null;
}

function BooleanRow({
  label,
  detail,
  field,
  value,
  actualValue,
  disabled,
  onChange,
}: {
  label: string;
  detail: string;
  field: SubagentField;
  value: SettingValue;
  actualValue: SettingValue;
  disabled: boolean;
  onChange: Props["onChange"];
}) {
  const { t } = useI18n();
  const groupName = `${field}-subagent-setting`;
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{label}</span>
          <span className="asb-app-setting-detail">{detail}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">{t("codex.subagentPanel.currentConfig", { value: actualValueLabel(actualValue, t) })}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label={label}>
          <RadioOption name={groupName} checked={value.mode === "automatic"} disabled={disabled}
            label={t("codex.subagent.auto")} onChange={() => onChange(field, automatic)} />
          <RadioOption name={groupName} checked={value.mode === "explicit" && value.value === true}
            disabled={disabled} label={t("codex.subagentPanel.on")} onChange={() => onChange(field, explicit(true))} />
          <RadioOption name={groupName} checked={value.mode === "explicit" && value.value === false}
            disabled={disabled} label={t("codex.subagentPanel.off")} onChange={() => onChange(field, explicit(false))} />
        </div>
      </div>
    </div>
  );
}

function SubagentModuleHeader({ headingId }: { headingId: string }) {
  const { t } = useI18n();
  return (
    <header className="asb-subagent-heading">
      <h3 id={headingId} className="asb-section-title">{t("codex.subagentPanel.title")}</h3>
    </header>
  );
}

function NumberRow({
  value,
  actualValue,
  disabled,
  onChange,
}: {
  value: SettingValue;
  actualValue: SettingValue;
  disabled: boolean;
  onChange: Props["onChange"];
}) {
  const { t } = useI18n();
  const modeName = "maxConcurrentThreadsPerSession-subagent-mode";
  const current = numericValue(value);
  const custom = value.mode === "explicit";
  const invalid = custom && (!Number.isSafeInteger(Number(current)) || Number(current) < 1 || !/^\d+$/.test(current));
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{t("codex.subagentPanel.concurrentLabel")}</span>
          <span className="asb-app-setting-detail">{t("codex.subagentPanel.concurrentDetail")}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">{t("codex.subagentPanel.currentConfig", { value: actualValueLabel(actualValue, t) })}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label={t("codex.subagentPanel.concurrentModeAria")}>
          <RadioOption name={modeName} checked={!custom} disabled={disabled} label={t("codex.subagent.auto")}
            onChange={() => onChange("maxConcurrentThreadsPerSession", automatic)} />
          <RadioOption name={modeName} checked={custom} disabled={disabled} label={t("codex.subagentPanel.specifyValue")}
            onChange={() => onChange("maxConcurrentThreadsPerSession", explicit(current))} />
        </div>
        {custom && (
          <div className="asb-subagent-input">
            <Input
              aria-invalid={invalid || undefined}
              aria-label={t("codex.subagentPanel.concurrentValueAria")}
              autoComplete="off"
              inputMode="numeric"
              disabled={disabled}
              value={current}
              onChange={(event) => {
                const raw = event.target.value;
                const parsed = Number(raw);
                onChange(
                  "maxConcurrentThreadsPerSession",
                  /^\d+$/.test(raw) && Number.isSafeInteger(parsed) ? explicit(parsed) : explicit(raw),
                );
              }}
            />
          </div>
        )}
      </div>
    </div>
  );
}

export function CodexSubagentSettingsPanel({
  editorState: state,
  busy,
  onChange,
  onRetryLoad,
}: Props) {
  const { t } = useI18n();
  const headingId = useId();
  const working = busy;
  const issue = state.draft ? draftIssue(state.draft, t) : null;

  if (state.phase === "idle" || state.phase === "loading") {
    return (
      <section className="asb-subagent-settings" aria-labelledby={headingId}>
        <SubagentModuleHeader headingId={headingId} />
        <div className="asb-settings-skeleton" role="status" aria-label={t("codex.loading")}>
          <div className="asb-skeleton" />
          <div className="asb-skeleton" />
          <div className="asb-skeleton" />
        </div>
      </section>
    );
  }

  if (state.phase === "loadError" || !state.snapshot || !state.draft) {
    return (
      <section className="asb-subagent-settings" aria-labelledby={headingId}>
        <SubagentModuleHeader headingId={headingId} />
        <div className="asb-empty" role="alert">
          <p>{t("codex.subagentPanel.loadError", { error: state.error ? commandErrorText(state.error, t) : t("codex.subagentPanel.configUnavailable") })}</p>
          <Button variant="secondary" disabled={busy} onClick={onRetryLoad}>{t("codex.subagentPanel.retryLoad")}</Button>
        </div>
      </section>
    );
  }

  return (
    <section className="asb-subagent-settings" aria-labelledby={headingId}>
      <SubagentModuleHeader headingId={headingId} />


      <div className="asb-subagent-setting-list">
        <BooleanRow
          label={t("codex.subagentPanel.enabledLabel")}
          detail={t("codex.subagentPanel.enabledDetail")}
          field="enabled"
          value={state.draft.enabled}
          actualValue={state.snapshot.settings.enabled}
          disabled={working}
          onChange={onChange}
        />
        <NumberRow
          value={state.draft.maxConcurrentThreadsPerSession}
          actualValue={state.snapshot.settings.maxConcurrentThreadsPerSession}
          disabled={working}
          onChange={onChange}
        />
        {issue && <p className="asb-field-error" role="alert">{issue}</p>}
        <BooleanRow
          label={t("codex.subagentPanel.interruptRowLabel")}
          detail={t("codex.subagentPanel.interruptDetail")}
          field="interruptMessage"
          value={state.draft.interruptMessage}
          actualValue={state.snapshot.settings.interruptMessage}
          disabled={working}
          onChange={onChange}
        />
      </div>
    </section>
  );
}
