import type { UpstreamProtocol } from "../../api/client";
import { Select } from "../Select";
import { ProviderCredentialField } from "../provider-editor/ProviderCredentialField";
import { ProviderEndpointField } from "../provider-editor/ProviderEndpointField";
import { reconcileCodexUpstream } from "./draft";
import type { CodexEditorState } from "./useCodexProviderEditor";

interface Props { editor: CodexEditorState; busy: boolean }

function ProtocolField({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  return (
    <label className="asb-field">
      <span>API 格式</span>
      <Select ariaLabel="API 格式" value={draft.upstream} disabled={busy}
        options={[
          { value: "responses", label: "Responses (/responses)" },
          { value: "chatCompletions", label: "Chat Completions (/chat/completions)" },
          { value: "anthropicMessages", label: "Anthropic Messages (/v1/messages)" },
        ]}
        onChange={(value) => setDraft((current) =>
          reconcileCodexUpstream(current, value as UpstreamProtocol))} />
    </label>
  );
}

export function CodexConnectionFields({ editor, busy }: Props) {
  const { draft, setDraft, connection } = editor;
  return (
    <section className="asb-provider-section" aria-label="连接配置">
      <h3 className="asb-section-title">连接配置</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-connection-choice">
          <ProtocolField editor={editor} busy={busy} />
        </div>
        <ProviderEndpointField busy={busy} baseUrl={draft.endpoint} protocol={draft.upstream}
          endpoints={connection.endpoints} endpointError={connection.endpointError}
          resolvingEndpoint={connection.resolvingEndpoint}
          onChange={(value) => setDraft((current) => ({ ...current, endpoint: value }))} />
        <ProviderCredentialField busy={busy} value={draft.apiKey} protocol={draft.upstream}
          onChange={(value) => setDraft((current) => ({ ...current, apiKey: value }))} />
        <div className="asb-provider-route-note">
          <p className="asb-scope-note asb-warn-text">{connection.gatewayRouteWarning}</p>
          {draft.upstream !== "responses" && (
            <p className="asb-scope-note asb-warn-text">
              此路由下 Codex 的网页搜索会关闭，client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游；需要这些能力请使用 Responses 上游。
            </p>
          )}
        </div>
      </div>
    </section>
  );
}
