import { useId } from "react";
import type { CodexSubagentSettings, SettingValue } from "../api/client";
import type { CodexSubagentSettingsEditorState } from "../app/useCodexSubagentSettings";
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

function actualValueLabel(value: SettingValue): string {
  if (value.mode === "automatic") return "自动";
  if (typeof value.value === "boolean") return value.value ? "开启" : "关闭";
  return String(value.value);
}

function draftIssue(settings: CodexSubagentSettings): string | null {
  const concurrency = settings.maxConcurrentThreadsPerSession;
  if (concurrency.mode === "explicit" &&
    (typeof concurrency.value !== "number" || !Number.isSafeInteger(concurrency.value) || concurrency.value < 1)) {
    return "最大并发必须是可准确表示且不小于 1 的整数，或改回自动。";
  }
  for (const [label, value] of [
    ["启用子 agent", settings.enabled],
    ["中断消息", settings.interruptMessage],
  ] as const) {
    if (value.mode === "explicit" && typeof value.value !== "boolean") {
      return `${label}只能设为开启、关闭或自动。`;
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
  const groupName = `${field}-subagent-setting`;
  return (
    <div className="asb-toggle-row asb-choice-row asb-subagent-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{label}</span>
          <span className="asb-app-setting-detail">{detail}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前配置：{actualValueLabel(actualValue)}</span>
      </div>
      <div className="asb-subagent-controls" role="radiogroup" aria-label={label}>
        <RadioOption name={groupName} checked={value.mode === "automatic"} disabled={disabled}
          label="自动" onChange={() => onChange(field, automatic)} />
        <RadioOption name={groupName} checked={value.mode === "explicit" && value.value === true}
          disabled={disabled} label="开启" onChange={() => onChange(field, explicit(true))} />
        <RadioOption name={groupName} checked={value.mode === "explicit" && value.value === false}
          disabled={disabled} label="关闭" onChange={() => onChange(field, explicit(false))} />
      </div>
    </div>
  );
}

function SubagentModuleHeader({ headingId }: { headingId: string }) {
  return (
    <header className="asb-subagent-heading">
      <h3 id={headingId} className="asb-section-title">子 agent 运行</h3>
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
  const modeName = "maxConcurrentThreadsPerSession-subagent-mode";
  const current = numericValue(value);
  const custom = value.mode === "explicit";
  const invalid = custom && (!Number.isSafeInteger(Number(current)) || Number(current) < 1 || !/^\d+$/.test(current));
  return (
    <div className="asb-toggle-row asb-choice-row asb-subagent-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">最大并发子 agent 线程数</span>
          <span className="asb-app-setting-detail">不设时由 Codex 决定本会话可同时运行的子 agent 数。</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前配置：{actualValueLabel(actualValue)}</span>
      </div>
      <div className="asb-subagent-controls" role="radiogroup" aria-label="最大并发子 agent 线程数配置方式">
        <RadioOption name={modeName} checked={!custom} disabled={disabled} label="自动"
          onChange={() => onChange("maxConcurrentThreadsPerSession", automatic)} />
        <RadioOption name={modeName} checked={custom} disabled={disabled} label="指定"
          onChange={() => onChange("maxConcurrentThreadsPerSession", explicit(current))} />
        {custom && (
          <div className="asb-subagent-input">
            <Input
              aria-invalid={invalid || undefined}
              aria-label="最大并发子 agent 线程数值"
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
  const headingId = useId();
  const working = busy;
  const issue = state.draft ? draftIssue(state.draft) : null;

  if (state.phase === "idle" || state.phase === "loading") {
    return (
      <section className="asb-subagent-settings" aria-labelledby={headingId}>
        <SubagentModuleHeader headingId={headingId} />
        <p className="asb-empty">正在读取子 agent 运行配置</p>
      </section>
    );
  }

  if (state.phase === "loadError" || !state.snapshot || !state.draft) {
    return (
      <section className="asb-subagent-settings" aria-labelledby={headingId}>
        <SubagentModuleHeader headingId={headingId} />
        <div className="asb-empty" role="alert">
          <p>无法读取子 agent 运行配置：{state.error?.message ?? "用户级配置不可用"}</p>
          <Button variant="secondary" disabled={busy} onClick={onRetryLoad}>重新读取</Button>
        </div>
      </section>
    );
  }

  return (
    <section className="asb-subagent-settings" aria-labelledby={headingId}>
      <SubagentModuleHeader headingId={headingId} />


      <div className="asb-subagent-setting-list">
        <BooleanRow
          label="启用子 agent"
          detail="控制 Codex 是否允许主 agent 创建子 agent。"
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
        <BooleanRow
          label="中断时发送消息"
          detail="控制主 agent 中断子 agent 时是否发送中断说明。"
          field="interruptMessage"
          value={state.draft.interruptMessage}
          actualValue={state.snapshot.settings.interruptMessage}
          disabled={working}
          onChange={onChange}
        />
      </div>

      {issue && <p className="asb-field-error" role="alert">{issue}</p>}
    </section>
  );
}
