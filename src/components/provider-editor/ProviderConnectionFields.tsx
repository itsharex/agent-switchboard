import { usesClaudeManagedAuth } from "../../api/claude-accounts";
import type { UpstreamProtocol } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { requiresGateway } from "../../lib/protocol";
import { Input } from "../Input";
import { Select } from "../Select";
import { ProviderCredentialField } from "./ProviderCredentialField";
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
          { value: "geminiGenerateContent", label: "Gemini Native (generateContent)" },
        ]}
        onChange={(value) => {
          const upstreamProtocol = value as UpstreamProtocol;
          setDraft((current) => ({
            ...current, upstreamProtocol,
            authentication: upstreamProtocol === "geminiGenerateContent" && current.authentication === "xApiKey"
              || upstreamProtocol !== "geminiGenerateContent" && current.authentication === "xGoogApiKey"
              ? null : current.authentication,
            responsesOptions: upstreamProtocol === "responses"
              ? { requestMode: "standard" } : null,
          }));
        }} />
    </label>
  );
}

function RouteNotice({ editor }: Pick<Props, "editor">) {
  const { draft, connection } = editor;
  return (
    <div className="asb-provider-route-note">
      {draft.connection?.claudeNative ? <p className="asb-scope-note">使用 Claude 原生 {draft.connection.claudeNative.kind} SDK；认证在“Claude 本地功能”中配置，不经过本机 HTTP 网关。</p> : requiresGateway(draft)
        ? <p className="asb-scope-note asb-warn-text">{connection.gatewayRouteWarning}</p>
        : <p className="asb-scope-note">{`与 ${clientName(draft.app)} 原生协议一致，切换后客户端直连所填服务地址。`}</p>}
    </div>
  );
}

export function ProviderConnectionFields({ editor, busy }: Props) {
  const { draft, setDraft, connection } = editor;
  return (
    <section className="asb-editor-section" aria-label="连接配置">
      <h3 className="asb-section-title">连接配置</h3>
      <div className="asb-editor-section-fields">
        <div className="asb-provider-connection-choice">
          <ProtocolField editor={editor} busy={busy || !!draft.connection?.claudeNative} />
        </div>
        <ProviderEndpointField busy={busy} required={!draft.connection?.claudeNative} baseUrl={draft.baseUrl} protocol={draft.upstreamProtocol}
          endpoints={connection.endpoints} endpointError={connection.endpointError}
          resolvingEndpoint={connection.resolvingEndpoint}
          onChange={(value) => setDraft((current) => ({ ...current, baseUrl: value }))} />
        <ProviderCredentialField key={`credential-${draft.app}`} busy={busy}
          value={draft.apiKey} protocol={draft.upstreamProtocol}
          authentication={draft.authentication}
          required={!usesClaudeManagedAuth(draft.connection) && !draft.connection?.claudeNative}
          onChange={(value) => setDraft((current) => ({ ...current, apiKey: value }))} />
        {!draft.connection?.claudeNative && !usesClaudeManagedAuth(draft.connection) && <label className="asb-field">
          <span>模型列表 URL</span>
          <Input type="url" value={draft.connection?.modelsUrl ?? ""} disabled={busy}
            placeholder={draft.connection?.isFullUrl ? "完整请求 URL 必填" : "（可选）"}
            onChange={(event) => setDraft((current) => ({
              ...current,
              connection: { ...current.connection, modelsUrl: event.target.value.trim() || null },
            }))} />
          <p className="asb-scope-note">仅用于“获取模型”。完整请求 URL 必须填写；不会改变实际请求地址。</p>
        </label>}
        <RouteNotice editor={editor} />
      </div>
    </section>
  );
}
