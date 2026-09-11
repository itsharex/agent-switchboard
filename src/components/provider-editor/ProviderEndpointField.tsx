import { useId } from "react";
import type { ProviderEndpoints, UpstreamProtocol } from "../../api/client";
import { Input } from "../Input";
import { PROTOCOL_NOTES } from "./draft";

interface Props {
  baseUrl: string | null;
  protocol: UpstreamProtocol | null;
  endpoints: ProviderEndpoints | null;
  endpointError: string | null;
  resolvingEndpoint: boolean;
  busy: boolean;
  onChange: (value: string) => void;
}

export function ProviderEndpointField({
  baseUrl, protocol, endpoints, endpointError, resolvingEndpoint, busy, onChange,
}: Props) {
  const urlId = useId();
  const helpId = `${urlId}-help`;
  const resultId = `${urlId}-result`;
  return (
    <div className="asb-field asb-provider-endpoint">
      <label htmlFor={urlId}>服务地址</label>
      <Input id={urlId} code type="url" required value={baseUrl ?? ""}
        disabled={busy} aria-describedby={`${helpId} ${resultId}`}
        aria-invalid={endpointError ? true : undefined}
        onChange={(event) => onChange(event.target.value)} />
      {protocol && <p className="asb-scope-note" id={helpId}>
        {PROTOCOL_NOTES[protocol]}
      </p>}
      <div id={resultId} aria-live="polite">
        {resolvingEndpoint && <p className="asb-scope-note">正在解析请求地址…</p>}
        {endpointError && <p className="asb-scope-note asb-warn-text" role="alert">{endpointError}</p>}
        {endpoints && <dl className="asb-provider-resolved-endpoint">
          <dt>请求地址</dt><dd>{endpoints.requestUrl}</dd>
        </dl>}
      </div>
    </div>
  );
}
