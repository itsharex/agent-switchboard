import { useState } from "react";
import type { ProviderDraft, ProviderRecord, SettingsValues } from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { OfficialLoginPanel } from "../OfficialLoginPanel";
import { ProviderNotesField } from "../provider-editor/ProviderIdentityFields";

interface Props {
  busy: boolean;
  /** The stored official-login record, or null while it is being created. */
  record: ProviderRecord | null;
  parameters: SettingsValues;
  onSave: (draft: ProviderDraft) => void;
  onCancel: () => void;
}

/** The Codex official-login arm of the editor. It owns a plain generic
 * `ProviderDraft`: an official-login record has no endpoint, credential, or
 * catalog, so it never touches the strict third-party contract. */
export function CodexOfficialProviderForm({ busy, record, parameters, onSave, onCancel }: Props) {
  const [name, setName] = useState(record?.profile.name ?? "Codex 官方登录");
  const [websiteUrl, setWebsiteUrl] = useState(record?.profile.websiteUrl ?? "");
  const [notes, setNotes] = useState(record?.profile.notes ?? "");
  const [quotaMinutes, setQuotaMinutes] = useState(
    record?.profile.officialQuotaRefreshIntervalMinutes ?? 0,
  );

  const save = () => {
    if (busy) return;
    onSave({
      app: "codex",
      routeMode: "official",
      name: name.trim(),
      baseUrl: null,
      apiKey: "",
      upstreamProtocol: null,
      responsesOptions: null,
      maxOutputTokens: null,
      model: null,
      modelOptions: null,
      parameters,
      notes: notes.trim() || null,
      websiteUrl: websiteUrl.trim() || null,
      officialQuotaRefreshIntervalMinutes: quotaMinutes > 0 ? quotaMinutes : null,
    });
  };

  const canSave = name.trim().length > 0 && !busy;

  return (
    <form className="asb-provider-form" aria-label={record ? "编辑 Codex 官方登录" : "新建 Codex 官方登录"}
      onSubmit={(event) => { event.preventDefault(); save(); }}>
      <section className="asb-provider-section" aria-label="基本资料">
        <h3 className="asb-section-title">基本资料</h3>
        <div className="asb-provider-section-fields">
          <div className="asb-provider-field-grid">
            <label className="asb-field">
              <span>名称</span>
              <Input value={name} required disabled={busy}
                onChange={(event) => setName(event.target.value)} />
            </label>
            <label className="asb-field">
              <span>官网地址</span>
              <Input type="url" value={websiteUrl} disabled={busy} placeholder="（可选）"
                onChange={(event) => setWebsiteUrl(event.target.value)} />
            </label>
          </div>
        </div>
      </section>
      <section className="asb-provider-section" aria-label="官方登录">
        <h3 className="asb-section-title">官方登录</h3>
        <div className="asb-provider-section-fields">
          <OfficialLoginPanel app="codex" />
        </div>
      </section>
      <section className="asb-provider-section" aria-label="订阅额度">
        <h3 className="asb-section-title">订阅额度</h3>
        <div className="asb-provider-section-fields">
          <div className="asb-provider-field-grid">
            <label className="asb-field">
              <span>自动刷新间隔（分钟）</span>
              <Input type="number" min="0" step="1" value={String(quotaMinutes)} disabled={busy}
                onChange={(event) => {
                  const parsed = Number(event.target.value.trim());
                  setQuotaMinutes(Number.isSafeInteger(parsed) && parsed > 0 ? parsed : 0);
                }} />
            </label>
          </div>
          <p className="asb-scope-note">0 表示不自动刷新；订阅额度读取本机 Codex 官方登录状态，不代表第三方用量。</p>
        </div>
      </section>
      <ProviderNotesField busy={busy} value={notes} onChange={setNotes} />
      <footer className="asb-provider-form-footer">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消</Button>
        <Button type="submit" variant="primary" disabled={!canSave}>保存供应商</Button>
      </footer>
    </form>
  );
}
