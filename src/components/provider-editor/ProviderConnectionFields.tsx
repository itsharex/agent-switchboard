import { useEffect, useState } from "react";
import type { UpstreamProtocol } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { requiresGateway } from "../../lib/protocol";
import { Button } from "../Button";
import { EyeOffIcon, PreviewIcon } from "../icons";
import { Input } from "../Input";
import { Select } from "../Select";
import { PROTOCOL_AUTHENTICATION_NOTES } from "./draft";
import { ProviderEndpointField } from "./ProviderEndpointField";
import type { ProviderEditorState } from "./useProviderEditor";

interface Props { editor: ProviderEditorState; busy: boolean }

function ProtocolField({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  return (
    <label className="asb-field">
      <span>API 格式</span>
      <Select ariaLabel="API 格式" value={draft.upstreamProtocol} disabled={busy}
        options={[
          { value: "anthropicMessages", label: "Anthropic Messages (/v1/messages)" },
          { value: "chatCompletions", label: "Chat Completions (/chat/completions)" },
          { value: "responses", label: "Responses (/responses)" },
        ]}
        onChange={(value) => {
          const upstreamProtocol = value as UpstreamProtocol;
          setDraft((current) => ({
            ...current, upstreamProtocol,
            responsesOptions: upstreamProtocol === "responses"
              ? { requestMode: "standard" } : null,
            maxOutputTokens: current.app === "codex" && upstreamProtocol === "anthropicMessages"
              ? (current.maxOutputTokens ?? 8192) : null,
          }));
        }} />
    </label>
  );
}

function RouteNotice({ editor }: Pick<Props, "editor">) {
  const { draft, connection } = editor;
  return (
    <div className="asb-provider-route-note">
      {requiresGateway(draft)
        ? <p className="asb-scope-note asb-warn-text">{connection.gatewayRouteWarning}</p>
        : <p className="asb-scope-note">{`与 ${clientName(draft.app)} 原生协议一致，切换后客户端直连所填服务地址。`}</p>}
      {draft.app === "codex" && draft.upstreamProtocol !== "responses" && (
        <p className="asb-scope-note asb-warn-text">
          此路由下 Codex 的网页搜索会关闭，client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游；需要这些能力请使用 Responses 上游。
        </p>
      )}
    </div>
  );
}

function OutputLimitField({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  if (draft.app !== "codex" || draft.upstreamProtocol !== "anthropicMessages") return null;
  return (
    <label className="asb-field">
      <span>最大输出 Token</span>
      <Input aria-label="最大输出 Token" type="number" min="1" step="1" required
        value={draft.maxOutputTokens?.toString() ?? ""} disabled={busy}
        onChange={(event) => {
          const value = event.target.value.trim();
          const parsed = Number(value);
          setDraft((current) => ({ ...current,
            maxOutputTokens: value && Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null,
          }));
        }} />
      <p className="asb-scope-note">Codex 未发送单次上限时，网关将使用此值构造 Anthropic 请求。</p>
    </label>
  );
}

function CredentialField({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  useEffect(() => setApiKeyVisible(false), [draft.app]);
  return (
    <div className="asb-field">
        <span>API 密钥</span>
        <div className="asb-secret-control">
          <Input aria-label="API 密钥" type={apiKeyVisible ? "text" : "password"} required
            value={draft.apiKey} disabled={busy}
            onChange={(event) => setDraft((current) => ({ ...current, apiKey: event.target.value }))} />
          <Button variant="secondary" aria-pressed={apiKeyVisible} disabled={busy}
            onClick={() => setApiKeyVisible((current) => !current)}>
            {apiKeyVisible ? <EyeOffIcon size={16} /> : <PreviewIcon size={16} />}
            {apiKeyVisible ? "隐藏密钥" : "查看密钥"}
          </Button>
        </div>
        {draft.upstreamProtocol && <p className="asb-scope-note">
          {PROTOCOL_AUTHENTICATION_NOTES[draft.upstreamProtocol]}
        </p>}
      </div>
  );
}

export function ProviderConnectionFields({ editor, busy }: Props) {
  return (
    <section className="asb-provider-section" aria-label="连接配置">
      <h3>连接配置</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-connection-choice">
          <ProtocolField editor={editor} busy={busy} />
          <OutputLimitField editor={editor} busy={busy} />
        </div>
        <ProviderEndpointField editor={editor} busy={busy} />
        <CredentialField editor={editor} busy={busy} />
        <RouteNotice editor={editor} />
      </div>
    </section>
  );
}
