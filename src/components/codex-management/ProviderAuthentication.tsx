import { useEffect, useRef, useState } from "react";
import { prepareCodexProfileSave, commitCodexProfileSave, type CodexProviderRecord, type CodexProviderDraft, type ProfileSavePreparation } from "../../api/providers";
import { cancelCodexProfileSave } from "../../api/codex-management";
import { Button } from "../Button";
import { Select } from "../Select";
import { PreviewInspector } from "../PreviewInspector";
import type { CodexOperations } from "./operations";
export function authenticationDraft(record: CodexProviderRecord, authentication: "bearer" | "xApiKey" | null): CodexProviderDraft {
  const { id: _id, routeMode: _mode, ...profile } = record.profile;
  return { ...profile, authentication, parameters: record.parameters, notes: record.notes, websiteUrl: record.websiteUrl, usageQuery: record.usageQuery };
}
export function ProviderAuthentication({ record, operations: { run, busy, changed }, onClose, onSaved }: { record: CodexProviderRecord; operations: CodexOperations; onClose: () => void; onSaved: () => Promise<void> }) {
  const [scheme, setScheme] = useState<"bearer" | "xApiKey" | null>(record.profile.authentication ?? null);
  const [prepared, setPrepared] = useState<ProfileSavePreparation | null>(null);
  const consumed = useRef(new Set<string>());
  useEffect(() => () => { if (prepared && !consumed.current.has(prepared.preparationId)) void cancelCodexProfileSave(prepared.preparationId).catch(() => {}); }, [prepared]);
  return <section className="asb-provider-section-fields" aria-label="Codex 上游认证设置">
    <h4 className="asb-group-title">{record.profile.name} · 上游认证</h4>
    <Select ariaLabel="Codex 上游认证方式" value={scheme ?? "automatic"} disabled={busy || !!prepared}
      options={[{ value: "automatic", label: "按上游协议默认" }, { value: "bearer", label: "Authorization: Bearer" }, { value: "xApiKey", label: "x-api-key" }]}
      onChange={(value) => setScheme(value === "automatic" ? null : value as "bearer" | "xApiKey")} />
    <p className="asb-scope-note">只改变此 Codex 档案的认证头，不改变 API 密钥、模型目录、通用配置或 Claude 设置。活动档案会先预览，再确认保存并应用。</p>
    {!prepared && <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setPrepared(await prepareCodexProfileSave(record.profile.id, authenticationDraft(record, scheme), record.fileHash)))}>准备认证变更</Button>}
    {prepared?.preview && <PreviewInspector filePreview={prepared.preview} userConfigModel={null} userConfigWarnings={[]} />}
    <div className="asb-form-actions"><Button variant="secondary" disabled={busy} onClick={onClose}>取消认证变更</Button>
      {prepared && <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
        const pending = prepared; consumed.current.add(pending.preparationId); setPrepared(null);
        await commitCodexProfileSave(pending.preparationId, true); changed("Codex 上游认证已保存。"); onClose(); await onSaved();
      })}>{prepared.kind === "saveAndApply" ? "确认保存并应用认证" : "确认保存认证"}</Button>}</div>
  </section>;
}
