import type { UpstreamProtocol } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { requiresGateway } from "../../lib/protocol";
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
        ]}
        onChange={(value) => {
          const upstreamProtocol = value as UpstreamProtocol;
          setDraft((current) => ({
            ...current, upstreamProtocol,
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
      {requiresGateway(draft)
        ? <p className="asb-scope-note asb-warn-text">{connection.gatewayRouteWarning}</p>
        : <p className="asb-scope-note">{`与 ${clientName(draft.app)} 原生协议一致，切换后客户端直连所填服务地址。`}</p>}
    </div>
  );
}

export function ProviderConnectionFields({ editor, busy }: Props) {
  const { draft, setDraft, connection } = editor;
  return (
    <section className="asb-provider-section" aria-label="连接配置">
      <h3 className="asb-section-title">连接配置</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-connection-choice">
          <ProtocolField editor={editor} busy={busy} />
        </div>
        <ProviderEndpointField busy={busy} baseUrl={draft.baseUrl} protocol={draft.upstreamProtocol}
          endpoints={connection.endpoints} endpointError={connection.endpointError}
          resolvingEndpoint={connection.resolvingEndpoint}
          onChange={(value) => setDraft((current) => ({ ...current, baseUrl: value }))} />
        <ProviderCredentialField key={`credential-${draft.app}`} busy={busy}
          value={draft.apiKey} protocol={draft.upstreamProtocol}
          onChange={(value) => setDraft((current) => ({ ...current, apiKey: value }))} />
        <RouteNotice editor={editor} />
      </div>
    </section>
  );
}
