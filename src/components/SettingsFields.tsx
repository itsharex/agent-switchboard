import type { AppKind, KeyChange, SettingSpec, SettingValue } from "../api/client";
import { Button } from "./Button";
import { DiffView } from "./DiffView";
import { RadioOption } from "./RadioOption";
import { Slider } from "./Slider";

interface Props {
  specs: SettingSpec[];
  groups: string[];
  values: Record<string, SettingValue>;
  /** Saved values used to summarize the current setting and derive a pending
   * change in client-preference and provider-parameter presentations. */
  baselineValues?: Record<string, SettingValue>;
  /** Read-only values currently present in the real client file. */
  actualValues?: Record<string, SettingValue>;
  busy: boolean;
  onChange: (key: string, value: SettingValue) => void;
  onResetGroup?: (group: string | null) => void;
  showGroupReset?: boolean;
  /** Client preferences and provider parameters share the richer decision-row
   * anatomy; all other catalog consumers retain compact form rows. */
  presentation?: "client" | "provider";
  /** The official client file whose current managed field is shown beside a
   * client-preference control. */
  clientApp?: AppKind;
}

const automatic: SettingValue = { mode: "automatic" };
function explicit(value: boolean | string): SettingValue { return { mode: "explicit", value }; }

function choiceIndex(spec: SettingSpec, value: SettingValue): number {
  return value.mode === "automatic" ? 0 : spec.options.findIndex((option) => option.value === value.value) + 1;
}

export function choiceLabel(spec: SettingSpec, value: SettingValue): string {
  return value.mode === "automatic" ? "自动" : spec.options.find((option) => option.value === value.value)?.label ?? String(value.value);
}

export function sameSettingValue(left: SettingValue, right: SettingValue): boolean {
  return left.mode === right.mode &&
    (left.mode === "automatic" || (right.mode === "explicit" && left.value === right.value));
}

function diffValue(value: SettingValue): string | null {
  return value.mode === "automatic" ? null : String(value.value);
}

function actualValueLabel(spec: SettingSpec, value: SettingValue | undefined): string {
  if (!value) return "真实文件：不可读取";
  return `真实文件：${choiceLabel(spec, value)}`;
}

function settingChange(
  spec: SettingSpec,
  baselineValue: SettingValue | undefined,
  value: SettingValue,
): KeyChange | null {
  if (!baselineValue || sameSettingValue(baselineValue, value)) return null;
  return {
    key: spec.key,
    kind: value.mode === "automatic" ? "remove" : "set",
    before: diffValue(baselineValue),
    after: diffValue(value),
  };
}

interface ControlProps {
  spec: SettingSpec;
  value: SettingValue;
  baselineValue?: SettingValue;
  actualValue?: SettingValue;
  showActual: boolean;
  busy: boolean;
  onChange: (value: SettingValue) => void;
  presentation?: "client" | "provider";
  clientApp?: AppKind;
}

type SettingControlProps = Pick<ControlProps, "spec" | "value" | "busy" | "onChange">;
type ProviderParameterRowProps = Pick<ControlProps, "spec" | "value" | "baselineValue" | "busy" | "onChange">;

function SettingControl({ spec, value, busy, onChange }: SettingControlProps) {
  if (spec.control === "slider") {
    return (
      <div className="asb-slider-control">
        <span className="asb-choice-current" aria-live="polite">当前推理：{choiceLabel(spec, value)}</span>
        <Slider value={choiceIndex(spec, value)} min={0} max={spec.options.length} step={1}
          ariaLabel={spec.label} ariaValueText={`${spec.label} ${choiceLabel(spec, value)}`} disabled={busy}
          onValueChange={(index) => onChange(index === 0 ? automatic : explicit(spec.options[index - 1].value))} />
      </div>
    );
  }
  const options = spec.control === "toggle"
    ? [{ value: true, label: "开启" }, { value: false, label: "关闭" }]
    : spec.options;
  return (
    <div className="asb-segments" role="radiogroup" aria-label={spec.label}>
      <RadioOption name={`${spec.key}-setting`} checked={value.mode === "automatic"} disabled={busy}
        label="自动" onChange={() => onChange(automatic)} />
      {options.map((option) => (
        <RadioOption key={String(option.value)} name={`${spec.key}-setting`}
          checked={value.mode === "explicit" && value.value === option.value}
          disabled={busy} label={option.label} onChange={() => onChange(explicit(option.value))} />
      ))}
    </div>
  );
}

function preferenceDetail(spec: SettingSpec): string {
  if (spec.control === "toggle") return "自动时不写入此项；开启或关闭会在预览确认后写入客户端配置。";
  if (spec.control === "slider") return "自动时遵循客户端默认等级；选择等级后会在预览确认后写入客户端配置。";
  return "自动时不写入此项；选择具体模式后会在预览确认后写入客户端配置。";
}

function providerParameterDetail(spec: SettingSpec): string {
  if (spec.control === "toggle") return "自动时遵循此供应商默认行为；开启或关闭会随供应商一起保存。";
  if (spec.control === "slider") return "自动时遵循此供应商默认等级；选择等级后会随供应商一起保存。";
  return "自动时遵循此供应商默认模式；选择具体模式后会随供应商一起保存。";
}

function pendingSettingMessage(spec: SettingSpec, value: SettingValue, action: "应用" | "保存"): string {
  return value.mode === "automatic"
    ? `待${action}：移除此项，恢复默认值`
    : `待${action}：${spec.control === "toggle" ? `设为${choiceLabel(spec, value)}` : `写入「${choiceLabel(spec, value)}」`}`;
}

function scalarCode(value: boolean | string | number): string {
  return typeof value === "string" ? JSON.stringify(value) : String(value);
}

function settingCode(app: AppKind, key: string, value: SettingValue | undefined): string {
  if (!value || value.mode === "automatic") return `# 未设置 ${key}`;
  const path = key.split(".");
  if (app === "codex") {
    const property = path.at(-1)!;
    const table = path.slice(0, -1);
    return `${table.length > 0 ? `[${table.join(".")}]\n` : ""}${property} = ${scalarCode(value.value)}`;
  }
  const root: Record<string, unknown> = {};
  let target = root;
  for (const segment of path.slice(0, -1)) {
    const nested: Record<string, unknown> = {};
    target[segment] = nested;
    target = nested;
  }
  target[path.at(-1)!] = value.value;
  return JSON.stringify(root, null, 2);
}

function prefixedCode(prefix: string, content: string): string {
  return content.split("\n").map((line) => `${prefix} ${line}`).join("\n");
}

function SettingCodeDisclosure({
  app,
  spec,
  actualValue,
  draftValue,
  changed,
  available,
}: {
  app: AppKind;
  spec: SettingSpec;
  actualValue: SettingValue | undefined;
  draftValue: SettingValue;
  changed: boolean;
  available: boolean;
}) {
  const actualCode = available ? settingCode(app, spec.key, actualValue) : `# 无法读取 ${spec.key}`;
  const draftCode = settingCode(app, spec.key, draftValue);
  const showDiff = changed && actualCode !== draftCode;
  return (
    <details className="asb-client-setting-code">
      <summary>{showDiff ? "查看待应用代码差异" : "查看当前配置代码"}</summary>
      {showDiff ? (
        <code aria-label={`${spec.label} 待应用代码差异`}>
          <span className="asb-client-setting-code-old">{prefixedCode("-", actualCode)}</span>
          {"\n"}
          <span className="asb-client-setting-code-new">{prefixedCode("+", draftCode)}</span>
        </code>
      ) : (
        <code>{actualCode}</code>
      )}
    </details>
  );
}

function ClientPreferenceRow({ spec, value, baselineValue, actualValue, showActual, busy, onChange, clientApp }: ControlProps) {
  const change = settingChange(spec, baselineValue, value);
  if (!clientApp) throw new Error("Client preference rows require a client application.");
  const actualLabel = showActual ? choiceLabel(spec, actualValue ?? automatic) : "不可读取";
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{spec.label}</span>
          <span className="asb-app-setting-detail">{preferenceDetail(spec)}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前配置：{actualLabel}</span>
      </div>
      <div className="asb-choice-controls">
        <SettingControl spec={spec} value={value} busy={busy} onChange={onChange} />
      </div>
      {change && (
        <p className="asb-setting-pending" role="status">
          {pendingSettingMessage(spec, value, "应用")}
        </p>
      )}
      <SettingCodeDisclosure app={clientApp} spec={spec} actualValue={actualValue}
        draftValue={value} changed={change !== null} available={showActual} />
    </div>
  );
}

function ProviderParameterRow({ spec, value, baselineValue, busy, onChange }: ProviderParameterRowProps) {
  const baseline = baselineValue ?? automatic;
  const changed = !sameSettingValue(baseline, value);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{spec.label}</span>
          <span className="asb-app-setting-detail">{providerParameterDetail(spec)}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前设置：{choiceLabel(spec, baseline)}</span>
      </div>
      <div className="asb-choice-controls">
        <SettingControl spec={spec} value={value} busy={busy} onChange={onChange} />
      </div>
      {changed && <p className="asb-setting-pending" role="status">{pendingSettingMessage(spec, value, "保存")}</p>}
    </div>
  );
}

/** One catalog-driven settings row. */
export function SettingsRow({
  spec,
  value,
  baselineValue,
  actualValue,
  showActual,
  busy,
  onChange,
  presentation,
  clientApp,
}: ControlProps) {
  if (presentation === "client") return <ClientPreferenceRow spec={spec} value={value} baselineValue={baselineValue}
    actualValue={actualValue} showActual={showActual} busy={busy} onChange={onChange} clientApp={clientApp} />;
  if (presentation === "provider") return <ProviderParameterRow spec={spec} value={value}
    baselineValue={baselineValue} busy={busy} onChange={onChange} />;
  const change = settingChange(spec, baselineValue, value);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <span className="asb-checkbox-label">{spec.label}</span>
        {showActual && <span className="asb-setting-actual" aria-live="polite">{actualValueLabel(spec, actualValue)}</span>}
      </div>
      <SettingControl spec={spec} value={value} busy={busy} onChange={onChange} />
      {change && (
        <div className="asb-setting-diff">
          <DiffView changes={[change]} label={`${spec.label} 未保存差异`} />
        </div>
      )}
    </div>
  );
}

/** Both ownership domains edit catalog values through the same controls. */
export function SettingsFields({
  specs,
  groups,
  values,
  baselineValues,
  actualValues,
  busy,
  onChange,
  onResetGroup,
  showGroupReset = true,
  presentation,
  clientApp,
}: Props) {
  return (
    <div className="asb-toggle-list">
      {groups.map((group) => {
        const groupSpecs = specs.filter((spec) => spec.group === group);
        if (!groupSpecs.length) return null;
        return (
          <section className="asb-toggle-group" key={group}>
            <div className="asb-toggle-group-head">
              <h3 className="asb-section-title">{group}</h3>
              {showGroupReset && onResetGroup && (
                <Button variant="secondary" disabled={busy} onClick={() => onResetGroup(group)}>恢复默认值</Button>
              )}
            </div>
            {groupSpecs.map((spec) => (
              <SettingsRow key={spec.key} spec={spec} value={values[spec.key]}
                baselineValue={baselineValues?.[spec.key]} actualValue={actualValues?.[spec.key]}
                showActual={actualValues !== undefined} busy={busy} presentation={presentation}
                clientApp={clientApp} onChange={(next) => onChange(spec.key, next)} />
            ))}
          </section>
        );
      })}
    </div>
  );
}
