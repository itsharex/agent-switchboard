import type { AppKind } from "../../api/client";
import { ClientLogo } from "../ClientLogo";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import type { CodexEditorDraft } from "./draft";
import type { SetCodexDraft } from "./useCodexProviderEditor";

interface IdentityProps {
  draft: CodexEditorDraft;
  busy: boolean;
  editing: boolean;
  setDraft: SetCodexDraft;
  /** Switching clients starts the other client's editor instead of mutating
   * this draft. */
  onSwitchClient: (app: AppKind) => void;
}

interface AccessModeProps {
  busy: boolean;
  onSwitchAccessMode: (official: boolean) => void;
}

export function CodexAccessMode({ busy, onSwitchAccessMode }: AccessModeProps) {
  return (
    <section className="asb-provider-section" aria-label="接入方式">
      <h3 className="asb-section-title">接入方式</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-field">
          <span>选择连接类型</span>
          <div className="asb-segments" role="radiogroup" aria-label="接入方式">
            <RadioOption name="codex-access-mode" checked label="第三方服务"
              disabled={busy} onChange={() => onSwitchAccessMode(false)} />
            <RadioOption name="codex-access-mode" checked={false} label="官方登录"
              disabled={busy} onChange={() => onSwitchAccessMode(true)} />
          </div>
        </div>
      </div>
    </section>
  );
}

function ClientField({ draft, busy, editing, onSwitchClient }: IdentityProps) {
  if (editing) {
    return <>
      <div className="asb-field"><span>客户端</span>
        <p className="asb-provider-identity-value"><ClientLogo app="codex" className="asb-edit-logo" />Codex</p>
      </div>
      <div className="asb-field"><span>接入方式</span>
        <p className="asb-provider-identity-value">第三方服务</p>
      </div>
    </>;
  }
  return <label className="asb-field"><span>客户端</span>
    <div className="asb-client-control">
      <ClientLogo app="codex" className="asb-edit-logo" />
      <Select ariaLabel="客户端" value="codex" disabled={busy}
        options={[{ value: "codex", label: "Codex" }, { value: "claude", label: "Claude" }]}
        onChange={(app) => { if (app !== "codex") onSwitchClient(app as AppKind); }} />
    </div>
  </label>;
}

export function CodexIdentityFields(props: IdentityProps) {
  const { draft, busy, setDraft } = props;
  return (
    <section className="asb-provider-section" aria-label="基本资料">
      <h3 className="asb-section-title">基本资料</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-field-grid"><ClientField {...props} /></div>
        <div className="asb-provider-field-grid">
          <label className="asb-field"><span>名称</span>
            <Input value={draft.name} required disabled={busy}
              onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))} />
          </label>
          <label className="asb-field"><span>官网地址</span>
            <Input type="url" value={draft.websiteUrl} disabled={busy} placeholder="（可选）"
              onChange={(event) => setDraft((current) => ({ ...current, websiteUrl: event.target.value }))} />
          </label>
        </div>
      </div>
    </section>
  );
}
