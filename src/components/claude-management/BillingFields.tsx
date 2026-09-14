import type { ClaudeBilling } from "../../api/claude-gateway";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Select } from "../Select";
const DEFAULT: ClaudeBilling = { costMultiplier: "1", modelSource: "response", dailyLimitUsd: null, monthlyLimitUsd: null };
export function BillingFields({ value, disabled, onChange }: {
  value: ClaudeBilling | null; disabled: boolean; onChange: (value: ClaudeBilling | null) => void;
}) {
  return <section aria-label="Claude 供应商计费策略" className="asb-provider-section-fields">
    <Checkbox label="启用此供应商请求计费与预算" checked={value !== null} disabled={disabled} onChange={(enabled) => onChange(enabled ? { ...DEFAULT } : null)} />
    {value && <>
      <Select ariaLabel="Claude 计价模型来源" value={value.modelSource} disabled={disabled}
        options={[{ value: "response", label: "上游实际返回模型" }, { value: "request", label: "请求映射后的模型" }]}
        onChange={(source) => onChange({ ...value, modelSource: source as ClaudeBilling["modelSource"] })} />
      {([['costMultiplier', '费用倍率'], ['dailyLimitUsd', '每日预算（USD）'], ['monthlyLimitUsd', '每月预算（USD）']] as const).map(([key, label]) =>
        <label className="asb-field" key={key}><span>{label}</span><Input inputMode="decimal" value={value[key] ?? ""} disabled={disabled}
          onChange={(e) => onChange({ ...value, [key]: key === "costMultiplier" ? e.target.value : e.target.value || null })} /></label>)}
      <p className="asb-scope-note">预算依据本机网关账本，不等于账户余额。未知模型价格或用量不会当作免费请求。</p>
    </>}
  </section>;
}
