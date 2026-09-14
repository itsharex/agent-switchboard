import { useState } from "react";
import { prepareClaudePreset, type ClaudePresetSummary, type ClaudePresetPreparation } from "../../api/claude-providers";
import { prepareProfileSave, commitProfileSave, type ProfileSavePreparation } from "../../api/providers";
import type { ClaudeAccountView } from "../../api/claude-accounts";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { ProfileSave } from "./ProfileSave";
import type { ClaudeOperations } from "./operations";
export function PresetForm({ presets, accounts, operations: op, onSaved }: {
  presets: ClaudePresetSummary[]; accounts: ClaudeAccountView[]; operations: ClaudeOperations; onSaved: () => Promise<void>;
}) {
  const [id, setId] = useState<string | null>(null);
  const [key, setKey] = useState("");
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [account, setAccount] = useState("default");
  const [prepared, setPrepared] = useState<ClaudePresetPreparation | null>(null);
  const [save, setSave] = useState<ProfileSavePreparation | null>(null);
  const preset = presets.find((p) => p.id === id);
  const disabled = op.busy || !!save;
  const choose = (value: string) => {
    const item = presets.find((p) => p.id === value); setId(value); setKey(""); setAccount("default"); setPrepared(null);
    setVariables(Object.fromEntries(Object.entries(item?.variables ?? {}).map(([key, field]) => [key, field.defaultValue ?? ""])));
  };
  return <div className="asb-provider-section-fields">
    <Select ariaLabel="Claude 离线预设" value={id} disabled={disabled} placeholder="选择预设" onChange={choose}
      options={presets.map((p) => ({ value: p.id, label: p.name + (p.unavailableReason ? " · 暂不可用" : "") }))} />
    {preset?.unavailableReason && <p role="alert" className="asb-field-error">{preset.unavailableReason}</p>}
    {preset && !preset.unavailableReason && <>
      {preset.authentication === "api_key" && <label className="asb-field"><span>预设 API 密钥</span><Input type="password" value={key} disabled={disabled} onChange={(e) => { setKey(e.target.value); setPrepared(null); }} /></label>}
      {preset.authentication !== "api_key" && preset.authentication !== "official" && preset.authentication !== "native_sdk" && <Select ariaLabel="预设绑定 Claude 账号" value={account} disabled={disabled} onChange={(value) => { setAccount(value); setPrepared(null); }}
        options={[{ value: "default", label: "跟随此服务的默认账号" }, ...accounts.filter((a) => a.provider === preset.authentication).map((a) => ({ value: a.id, label: a.label }))]} />}
      {Object.entries(preset.variables).map(([name, field]) => <label className="asb-field" key={name}><span>{field.label}</span>
        <Input type={/SECRET|ACCESS_KEY/.test(name) ? "password" : "text"} value={variables[name] ?? ""} placeholder={field.placeholder} disabled={disabled}
          onChange={(e) => { setVariables({ ...variables, [name]: e.target.value }); setPrepared(null); }} /></label>)}
      <Button variant="secondary" disabled={disabled || preset.authentication === "api_key" && !key.trim()} onClick={() => void op.run(async () => {
        setPrepared(await prepareClaudePreset(preset.id, key, variables, account === "default" ? null : account));
      })}>准备 Claude 预设</Button>
    </>}
    {prepared && <section aria-label="Claude 预设准备结果" className="asb-provider-section-fields">
      <label className="asb-field"><span>档案名称</span><Input value={prepared.draft.name} disabled={disabled} onChange={(e) => setPrepared({ ...prepared, draft: { ...prepared.draft, name: e.target.value } })} /></label>
      <p className="asb-scope-note">{prepared.draft.baseUrl ?? "Claude 官方登录"}{prepared.draft.model ? " · " + prepared.draft.model : ""}</p>
      {prepared.warnings.map((warning, index) => <p className="asb-scope-note" key={index}>{warning}</p>)}
      <Button variant="primary" disabled={disabled || !prepared.draft.name.trim()} onClick={() => void op.run(async () => setSave(await prepareProfileSave(null, prepared.draft, null)))}>预览保存预设档案</Button>
    </section>}
    {save && <ProfileSave preparation={save} busy={op.busy} onCancel={() => setSave(null)} onConfirm={() => void op.run(async () => {
      await commitProfileSave(save.preparationId, true); setSave(null); setPrepared(null); setKey(""); await onSaved();
    })} />}
  </div>;
}
