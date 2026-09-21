import { Button } from "../Button";
import { Input } from "../Input";
import { OfficialLoginPanel } from "../OfficialLoginPanel";
import { ProviderAdvancedSettings } from "../provider-editor/ProviderAdvancedSettings";
import { ProviderNotesField } from "../provider-editor/ProviderIdentityFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import { RadioOption } from "../RadioOption";
import type { useCodexOfficialEditor } from "./useCodexOfficialEditor";

interface Props {
  formId: string;
  busy: boolean;
  editing: boolean;
  editor: ReturnType<typeof useCodexOfficialEditor>;
  onSubmit: () => void;
  onSwitchAccessMode: () => void;
}

function BasicDetails({ busy, editing, name, websiteUrl, onNameChange, onWebsiteChange, onSwitchAccessMode }: {
  busy: boolean;
  editing: boolean;
  name: string;
  websiteUrl: string;
  onNameChange: (value: string) => void;
  onWebsiteChange: (value: string) => void;
  onSwitchAccessMode: () => void;
}) {
  return <section className="asb-editor-section" aria-label="基本资料">
    <h3 className="asb-section-title">基本资料</h3>
    <div className="asb-editor-section-fields">
      <div className="asb-provider-field-grid">
        <div className="asb-field"><span>接入方式</span>
          {editing ? <p className="asb-provider-identity-value">官方登录</p> : <div className="asb-segments" role="radiogroup" aria-label="接入方式">
            <RadioOption name="codex-access-mode" checked={false} label="第三方服务"
              disabled={busy} onChange={onSwitchAccessMode} />
            <RadioOption name="codex-access-mode" checked label="官方登录" disabled={busy}
              onChange={() => undefined} />
          </div>}
        </div>
        <label className="asb-field"><span>名称</span>
          <Input value={name} required disabled={busy} onChange={(event) => onNameChange(event.target.value)} />
        </label>
      </div>
      <div className="asb-provider-field-grid">
        <label className="asb-field"><span>官网地址</span>
          <Input type="url" value={websiteUrl} disabled={busy} placeholder="（可选）"
            onChange={(event) => onWebsiteChange(event.target.value)} />
        </label>
      </div>
    </div>
  </section>;
}

function SubscriptionSettings({ busy, value, onChange }: {
  busy: boolean;
  value: number;
  onChange: (value: number) => void;
}) {
  return <div className="asb-provider-advanced-group">
    <label className="asb-field"><span>订阅额度自动刷新间隔（分钟）</span>
      <Input type="number" min="0" step="1" value={String(value)} disabled={busy}
        onChange={(event) => {
          const parsed = Number(event.target.value.trim());
          onChange(Number.isSafeInteger(parsed) && parsed > 0 ? parsed : 0);
        }} />
    </label>
    <p className="asb-scope-note">0 表示不自动刷新；订阅额度读取本机 Codex 官方登录状态，不代表第三方用量。</p>
  </div>;
}

/** The official session owns its draft, including provider runtime parameters. */
export function CodexOfficialProviderForm({
  formId,
  busy,
  editing,
  editor,
  onSubmit,
  onSwitchAccessMode,
}: Props) {
  const { draft, setDraft, parameters } = editor;
  return <form id={formId} className="asb-provider-form" aria-label={editing ? "编辑 Codex 官方登录" : "新建 Codex 官方登录"}
    onSubmit={(event) => { event.preventDefault(); onSubmit(); }}>
    <BasicDetails busy={busy} editing={editing} name={draft.name} websiteUrl={draft.websiteUrl ?? ""}
      onNameChange={(name) => setDraft((current) => ({ ...current, name }))}
      onWebsiteChange={(websiteUrl) => setDraft((current) => ({ ...current, websiteUrl }))}
      onSwitchAccessMode={onSwitchAccessMode} />
    <section className="asb-editor-section" aria-label="官方登录">
      <h3 className="asb-section-title">官方登录</h3>
      <div className="asb-editor-section-fields"><OfficialLoginPanel app="codex" /></div>
    </section>
    <ProviderAdvancedSettings>
      <div className="asb-provider-advanced-action">
        <div><strong>运行参数</strong><span>随此供应商保存，不直接写入客户端配置。</span></div>
        <Button ref={editor.triggerRef} variant="secondary" disabled={busy || !parameters.ready}
          onClick={() => editor.setParametersOpen(true)}>配置运行参数 <span aria-hidden="true">→</span></Button>
      </div>
      <ParametersLoadStatus busy={busy} ready={parameters.ready} error={parameters.error} retry={parameters.retry} />
      <SubscriptionSettings busy={busy} value={draft.officialQuotaRefreshIntervalMinutes ?? 0}
        onChange={(value) => setDraft((current) => ({ ...current, officialQuotaRefreshIntervalMinutes: value > 0 ? value : null }))} />
      <ProviderNotesField busy={busy} value={draft.notes ?? ""}
        onChange={(notes) => setDraft((current) => ({ ...current, notes }))} />
    </ProviderAdvancedSettings>
  </form>;
}
