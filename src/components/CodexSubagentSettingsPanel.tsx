import { useId, useState } from "react";
import type { CodexSubagentSettings, SettingValue } from "../api/client";
import type { CodexSubagentSettingsEditorState } from "../app/useCodexSubagentSettings";
import { Button } from "./Button";
import { CodePreview } from "./CodePreview";
import { ConfirmSheet } from "./ConfirmSheet";
import { Input } from "./Input";
import { RadioOption } from "./RadioOption";

type SubagentField = keyof CodexSubagentSettings;

interface Props {
  editorState: CodexSubagentSettingsEditorState;
  busy: boolean;
  onChange: (field: SubagentField, value: SettingValue) => void;
  onReset: () => void;
  onRetryLoad: () => void;
  onPreview: () => void;
  onApplyPreview: () => Promise<boolean>;
}

const automatic: SettingValue = { mode: "automatic" };

function explicit(value: boolean | string | number): SettingValue {
  return { mode: "explicit", value };
}

function numericValue(value: SettingValue): string {
  return value.mode === "explicit" ? String(value.value) : "";
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
  disabled,
  onChange,
}: {
  label: string;
  detail: string;
  field: SubagentField;
  value: SettingValue;
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

function NumberRow({
  value,
  disabled,
  onChange,
}: {
  value: SettingValue;
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
  onReset,
  onRetryLoad,
  onPreview,
  onApplyPreview,
}: Props) {
  const [confirming, setConfirming] = useState(false);
  const headingId = useId();
  const working = busy || state.phase === "applying";
  const issue = state.draft ? draftIssue(state.draft) : null;
  const canPreview = (state.phase === "dirty" || state.phase === "applyError") && !state.preview && !state.previewing && !issue && !working;

  if (state.phase === "idle" || state.phase === "loading") {
    return (
      <section className="asb-toggle-group asb-subagent-settings" aria-labelledby={headingId}>
        <div className="asb-toggle-group-head">
          <h3 id={headingId} className="asb-section-title">子 agent 运行</h3>
        </div>
        <p className="asb-empty">正在读取 Codex 子 agent 设置</p>
      </section>
    );
  }

  if (state.phase === "loadError" || !state.snapshot || !state.draft) {
    return (
      <section className="asb-toggle-group asb-subagent-settings" aria-labelledby={headingId}>
        <div className="asb-toggle-group-head">
          <h3 id={headingId} className="asb-section-title">子 agent 运行</h3>
        </div>
        <div className="asb-empty" role="alert">
          <p>无法读取 Codex 子 agent 设置：{state.error?.message ?? "用户级配置不可用"}</p>
          <Button variant="secondary" disabled={busy} onClick={onRetryLoad}>重新读取</Button>
        </div>
      </section>
    );
  }

  const preview = state.preview;
  return (
    <section className="asb-toggle-group asb-subagent-settings" aria-labelledby={headingId}>
      <div className="asb-toggle-group-head">
        <div className="asb-subagent-heading">
          <h3 id={headingId} className="asb-section-title">子 agent 运行</h3>
          <p className="asb-field-help">这些设置管理 Codex 的全局运行策略；默认模型与推理强度请在对应供应商的“运行参数”中设置。</p>
        </div>
        <Button variant="secondary" disabled={working} onClick={onReset}>恢复 Codex 默认值</Button>
      </div>

      {state.snapshot.deprecatedKeys.length > 0 && (
        <p className="asb-field-error" role="alert">
          检测到已弃用的 {state.snapshot.deprecatedKeys.join("、")}。ASB 不会读取、转换或写入它；若要指定最大并发，请先在 Codex 中手动删除该键。
        </p>
      )}

      <BooleanRow
        label="启用子 agent"
        detail="控制 Codex 是否允许主 agent 创建子 agent。"
        field="enabled"
        value={state.draft.enabled}
        disabled={working}
        onChange={onChange}
      />
      <NumberRow
        value={state.draft.maxConcurrentThreadsPerSession}
        disabled={working}
        onChange={onChange}
      />
      <BooleanRow
        label="中断时发送消息"
        detail="控制主 agent 中断子 agent 时是否发送中断说明。"
        field="interruptMessage"
        value={state.draft.interruptMessage}
        disabled={working}
        onChange={onChange}
      />

      {issue && <p className="asb-field-error" role="alert">{issue}</p>}
      {state.error && state.phase === "applyError" && (
        <div className="asb-subagent-error-actions" role="alert">
          <p className="asb-field-error">应用失败，草稿已保留：{state.error.message}</p>
          <Button variant="secondary" disabled={working} onClick={onRetryLoad}>重新读取</Button>
        </div>
      )}
      {state.previewError && (
        <div className="asb-subagent-error-actions" role="alert">
          <p className="asb-field-error">无法生成预览：{state.previewError.message}</p>
          <Button variant="secondary" disabled={working} onClick={onRetryLoad}>重新读取</Button>
        </div>
      )}

      {!preview && (
        <div className="asb-form-actions">
          <Button variant="primary" disabled={!canPreview} onClick={onPreview}>
            {state.previewing ? "正在生成预览" : "生成写入预览"}
          </Button>
        </div>
      )}
      {preview && (
        <div className="asb-settings-preview">
          <CodePreview target={preview.target} content={preview.content} />
          <div className="asb-form-actions">
            <Button variant="primary" disabled={working} onClick={() => setConfirming(true)}>
              应用子 agent 设置
            </Button>
          </div>
        </div>
      )}

      {confirming && preview && (
        <ConfirmSheet
          title="确认应用子 agent 设置"
          details={[
            `将写入 ${preview.target}`,
            "只会变更本模块拥有的三个全局子 agent 键；供应商参数、角色表与其他用户配置保持不变。",
            <CodePreview target={preview.target} content={preview.content} />,
            "写入前会创建备份，并在校验后以可恢复事务替换文件。",
          ]}
          confirmLabel="确认应用"
          onConfirm={() => {
            setConfirming(false);
            void onApplyPreview();
          }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </section>
  );
}
