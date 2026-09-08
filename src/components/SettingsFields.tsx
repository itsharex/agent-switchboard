import type { SettingSpec, SettingValue } from "../api/client";
import { Button } from "./Button";
import { RadioOption } from "./RadioOption";
import { Slider } from "./Slider";

interface Props {
  specs: SettingSpec[];
  groups: string[];
  values: Record<string, SettingValue>;
  busy: boolean;
  onChange: (key: string, value: SettingValue) => void;
  onResetGroup: (group: string | null) => void;
}

const automatic: SettingValue = { mode: "automatic" };
function explicit(value: boolean | string): SettingValue { return { mode: "explicit", value }; }

function choiceIndex(spec: SettingSpec, value: SettingValue): number {
  return value.mode === "automatic" ? 0 : spec.options.findIndex((option) => option.value === value.value) + 1;
}

function choiceLabel(spec: SettingSpec, value: SettingValue): string {
  return value.mode === "automatic" ? "自动" : spec.options.find((option) => option.value === value.value)?.label ?? String(value.value);
}

interface ControlProps {
  spec: SettingSpec;
  value: SettingValue;
  busy: boolean;
  onChange: (value: SettingValue) => void;
}

function SettingControl({ spec, value, busy, onChange }: ControlProps) {
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

/** One catalog-driven settings row. Model pickers need the provider connection
 * and are rendered by the provider parameters page instead. */
export function SettingsRow({ spec, value, busy, onChange }: ControlProps) {
  if (spec.control === "model") return null;
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head"><span className="asb-checkbox-label">{spec.label}</span></div>
      <SettingControl spec={spec} value={value} busy={busy} onChange={onChange} />
    </div>
  );
}

/** Both ownership domains edit catalog values through the same controls. */
export function SettingsFields({ specs, groups, values, busy, onChange, onResetGroup }: Props) {
  return (
    <div className="asb-toggle-list">
      {groups.map((group) => {
        const groupSpecs = specs.filter((spec) => spec.group === group);
        if (!groupSpecs.length) return null;
        return (
          <section className="asb-toggle-group" key={group}>
            <div className="asb-toggle-group-head">
              <h3 className="asb-toggle-group-title">{group}</h3>
              <Button variant="secondary" disabled={busy} onClick={() => onResetGroup(group)}>恢复默认值</Button>
            </div>
            {groupSpecs.map((spec) => (
              <SettingsRow key={spec.key} spec={spec} value={values[spec.key]} busy={busy}
                onChange={(next) => onChange(spec.key, next)} />
            ))}
          </section>
        );
      })}
    </div>
  );
}
