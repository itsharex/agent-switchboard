import { Input } from "../Input";
import { PROTOCOL_NOTES } from "./draft";
import type { ProviderEditorState } from "./useProviderEditor";

export function ProviderEndpointField({ editor, busy }: { editor: ProviderEditorState; busy: boolean }) {
  const { draft, setDraft, connection } = editor;
  const { endpoints, endpointError, resolvingEndpoint } = connection;
  return (
    <div className="asb-field asb-provider-endpoint">
      <label htmlFor="provider-base-url">服务地址</label>
      <Input id="provider-base-url" code type="url" required value={draft.baseUrl ?? ""}
        disabled={busy} aria-describedby="provider-endpoint-help provider-endpoint-result"
        aria-invalid={endpointError ? true : undefined}
        onChange={(event) => setDraft((current) => ({ ...current, baseUrl: event.target.value }))} />
      {draft.upstreamProtocol && <p className="asb-scope-note" id="provider-endpoint-help">
        {PROTOCOL_NOTES[draft.upstreamProtocol]}
      </p>}
      <div id="provider-endpoint-result" aria-live="polite">
        {resolvingEndpoint && <p className="asb-scope-note">正在解析请求地址…</p>}
        {endpointError && <p className="asb-scope-note asb-warn-text" role="alert">{endpointError}</p>}
        {endpoints && <dl className="asb-provider-resolved-endpoint">
          <dt>请求地址</dt><dd>{endpoints.requestUrl}</dd>
        </dl>}
      </div>
    </div>
  );
}
