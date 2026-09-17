import { useState } from "react";
import type { AppKind } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { ClientLogo } from "../ClientLogo";
import { ChevronDownIcon } from "../icons";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import { Textarea } from "../Textarea";
import { defaultConnection } from "./draft";
import type { ProviderEditorState } from "./useProviderEditor";

interface IdentityProps {
  editor: ProviderEditorState;
  busy: boolean;
  editing: boolean;
  /** Switching to Codex replaces the editor session; a Claude draft never
   * mutates into a Codex draft. */
  onSwitchClient: (app: AppKind) => void;
}

interface AccessModeProps {
  editor: ProviderEditorState;
  busy: boolean;
  officialTakenApps: AppKind[];
  onOpenOfficial: (app: AppKind) => void;
}

export function ProviderAccessMode({ editor, busy, officialTakenApps, onOpenOfficial }: AccessModeProps) {
  const { draft, setDraft, setLoginDone } = editor;
  return (
    <section className="asb-provider-section" aria-label="接入方式">
      <h3 className="asb-section-title">接入方式</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-field">
          <span>选择连接类型</span>
          <div className="asb-segments" role="radiogroup" aria-label="接入方式">
            <RadioOption name="access-mode" checked={draft.routeMode === "custom"}
              disabled={busy} label="自定义 API 中继" onChange={() => {
                setDraft((current) => ({ ...current, routeMode: "custom", ...defaultConnection(current.app) }));
                setLoginDone(false);
              }} />
            <RadioOption name="access-mode" checked={draft.routeMode === "official"}
              disabled={busy} label="官方登录" onChange={() => {
                if (officialTakenApps.includes(draft.app)) { onOpenOfficial(draft.app); return; }
                setDraft((current) => ({
                  ...current,
                  routeMode: "official",
                  name: current.name.trim() || `${clientName(current.app)} 官方登录`,
                  model: null,
                  baseUrl: null,
                  apiKey: "",
                  upstreamProtocol: null,
                  authentication: null,
                  responsesOptions: null,
                  maxOutputTokens: null,
                  modelOptions: null,
                  usageQuery: null,
                  officialQuotaRefreshIntervalMinutes: null,
                }));
                setLoginDone(false);
              }} />
          </div>
        </div>
      </div>
    </section>
  );
}

function ClientField({ editor, busy, editing, onSwitchClient }: IdentityProps) {
  const { draft } = editor;
  if (editing) {
    return <>
      <div className="asb-field"><span>客户端</span>
        <p className="asb-provider-identity-value"><ClientLogo app={draft.app} className="asb-edit-logo" />{clientName(draft.app)}</p>
      </div>
      <div className="asb-field"><span>接入方式</span>
        <p className="asb-provider-identity-value">{draft.routeMode === "official" ? "官方登录" : "自定义 API 中继"}</p>
      </div>
    </>;
  }
  return <label className="asb-field"><span>客户端</span>
    <div className="asb-client-control">
      <ClientLogo app={draft.app} className="asb-edit-logo" />
      <Select ariaLabel="客户端" value={draft.app} disabled={busy}
        options={[{ value: "codex", label: "Codex" }, { value: "claude", label: "Claude" }]}
        onChange={(app) => { if (app !== draft.app) onSwitchClient(app as AppKind); }} />
    </div>
  </label>;
}

export function ProviderIdentityFields(props: IdentityProps) {
  const { editor, busy, editing } = props;
  const { draft, setDraft } = editor;
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
            <Input type="url" value={draft.websiteUrl ?? ""} disabled={busy} placeholder="（可选）"
              onChange={(event) => setDraft((current) => ({ ...current, websiteUrl: event.target.value }))} />
          </label>
        </div>
      </div>
    </section>
  );
}

export function ProviderNotesField({ value, busy, onChange }: {
  value: string | null;
  busy: boolean;
  onChange: (value: string) => void;
}) {
  const [expanded, setExpanded] = useState(() => Boolean(value?.trim()));
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>备注</span><span className="asb-provider-disclosure-value">{value?.trim() ? "已填写" : "可选"}</span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body">
        <Textarea aria-label="备注" rows={3} value={value ?? ""} disabled={busy}
          placeholder="仅保存在本应用，便于区分供应商"
          onChange={(event) => onChange(event.target.value)} />
      </div>
    </details>
  );
}
