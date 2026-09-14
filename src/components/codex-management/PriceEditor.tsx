import { useState } from "react";
import type { CodexMeteringSettings, CodexModelPrice, CodexBilling } from "../../api/codex-metering";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
const blankPrice = (): CodexModelPrice => ({ inputUsdPerMillion: "0", outputUsdPerMillion: "0", cacheReadUsdPerMillion: "0", cacheCreationUsdPerMillion: "0", source: "manual" });
const blankBilling = (): CodexBilling => ({ costMultiplier: "1", modelSource: "request", dailyLimitUsd: null, monthlyLimitUsd: null });
const FIELDS = [["inputUsdPerMillion", "输入价格"], ["outputUsdPerMillion", "输出价格"], ["cacheReadUsdPerMillion", "缓存读取价格"], ["cacheCreationUsdPerMillion", "缓存创建价格"]] as const;
export function PriceEditor({ settings, onChange, busy }: { settings: CodexMeteringSettings; onChange: (next: CodexMeteringSettings) => void; busy: boolean }) {
  const [model, setModel] = useState(""); const [price, setPrice] = useState<CodexModelPrice>(blankPrice);
  return <fieldset className="asb-fieldset" disabled={busy}><legend>本地模型定价（美元 / 百万 token）</legend>
    <Select ariaLabel="已有 Codex 模型价格" value={settings.prices[model] ? model : null} options={Object.keys(settings.prices).map((id) => ({ value: id, label: id }))} onChange={(id) => { setModel(id); setPrice(settings.prices[id]); }} />
    <label className="asb-field"><span>计价模型 ID</span><Input value={model} onChange={(e) => setModel(e.target.value)} /></label>
    {FIELDS.map(([field, label]) => <label className="asb-field" key={field}><span>{label}</span><Input inputMode="decimal" value={price[field]} onChange={(e) => setPrice({ ...price, [field]: e.target.value, source: "manual" })} /></label>)}
    <Button variant="secondary" disabled={!model.trim()} onClick={() => onChange({ ...settings, prices: { ...settings.prices, [model.trim()]: price } })}>加入定价草稿</Button>
    <Button variant="secondary" disabled={!settings.prices[model]} onClick={() => { const prices = { ...settings.prices }; delete prices[model]; onChange({ ...settings, prices }); }}>从草稿移除价格</Button>
  </fieldset>;
}
export function BillingEditor({ settings, onChange, providers, busy }: { settings: CodexMeteringSettings; onChange: (next: CodexMeteringSettings) => void; providers: Array<{ id: string; name: string }>; busy: boolean }) {
  const [id, setId] = useState<string | null>(null);
  const value = (id && settings.providers[id]) || blankBilling();
  const patch = (next: Partial<CodexBilling>) => { if (id) onChange({ ...settings, providers: { ...settings.providers, [id]: { ...value, ...next } } }); };
  return <fieldset className="asb-fieldset" disabled={busy}><legend>供应商倍率与预算</legend>
    <Select ariaLabel="Codex 计量供应商" value={id} options={providers.map((p) => ({ value: p.id, label: p.name }))} onChange={setId} />
    {id && <><label className="asb-field"><span>费用倍率</span><Input inputMode="decimal" value={value.costMultiplier} onChange={(e) => patch({ costMultiplier: e.target.value })} /></label>
      <Select ariaLabel="Codex 计价模型来源" value={value.modelSource} options={[{ value: "request", label: "实际出站请求模型" }, { value: "response", label: "实际响应模型" }]} onChange={(source) => patch({ modelSource: source as "request" | "response" })} />
      <label className="asb-field"><span>日预算（美元，留空停用）</span><Input inputMode="decimal" value={value.dailyLimitUsd ?? ""} onChange={(e) => patch({ dailyLimitUsd: e.target.value || null })} /></label>
      <label className="asb-field"><span>月预算（美元，留空停用）</span><Input inputMode="decimal" value={value.monthlyLimitUsd ?? ""} onChange={(e) => patch({ monthlyLimitUsd: e.target.value || null })} /></label></>}
  </fieldset>;
}
