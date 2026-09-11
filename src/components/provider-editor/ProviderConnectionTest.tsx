import { useId, useMemo, useRef, useState } from "react";
import type { ProviderRequestTarget, ResponsesOptions, UpstreamProtocol } from "../../api/client";
import { Button } from "../Button";
import { ConnectivityIcon } from "../icons";
import { ProviderTestPanel } from "../ProviderTestPanel";

interface Props {
  baseUrl: string | null;
  apiKey: string;
  upstreamProtocol: UpstreamProtocol | null;
  /** Required exactly when the protocol is Responses, matching the backend contract. */
  responsesOptions: ResponsesOptions | null;
  defaultModel: string | null;
  busy: boolean;
  active: boolean;
}

export function ProviderConnectionTest({
  baseUrl, apiKey, upstreamProtocol, responsesOptions, defaultModel, busy, active,
}: Props) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const valid = upstreamProtocol === "responses"
    ? responsesOptions !== null
    : responsesOptions === null;
  const target = useMemo<ProviderRequestTarget | null>(() =>
    baseUrl?.trim() && apiKey.trim() && upstreamProtocol && valid ? {
      kind: "draft", connection: { baseUrl: baseUrl.trim(), apiKey: apiKey.trim(),
        upstreamProtocol, responsesOptions, defaultModel: defaultModel?.trim() || null },
    } : null, [baseUrl, apiKey, upstreamProtocol, responsesOptions, defaultModel, valid]);
  return <section className="asb-provider-section" aria-label="连接测试">
    <h3 className="asb-section-title">连接测试</h3>
    <div className="asb-provider-section-fields">
      <div><Button ref={trigger} variant="secondary" disabled={busy} aria-expanded={open && active}
        aria-controls={id} onClick={() => setOpen((value) => !value)}>
        <ConnectivityIcon />{open ? "收起测试" : "测试供应商"}
      </Button></div>
      {open && active && <ProviderTestPanel id={id} name="当前草稿" url={baseUrl?.trim() || null} target={target}
        onClose={() => { setOpen(false); trigger.current?.focus(); }} />}
    </div>
  </section>;
}
