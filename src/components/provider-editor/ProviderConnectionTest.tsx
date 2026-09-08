import { useId, useMemo, useRef, useState } from "react";
import type { ProviderRequestTarget } from "../../api/client";
import { Button } from "../Button";
import { ConnectivityIcon } from "../icons";
import { ProviderTestPanel } from "../ProviderTestPanel";
import { responsesOptionsValid, type ProviderEditorDraft } from "./draft";

export function ProviderConnectionTest({ draft, busy, active }: {
  draft: ProviderEditorDraft; busy: boolean; active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const { baseUrl, apiKey, upstreamProtocol, responsesOptions, model } = draft;
  const valid = responsesOptionsValid(draft);
  const target = useMemo<ProviderRequestTarget | null>(() =>
    baseUrl?.trim() && apiKey.trim() && upstreamProtocol && valid ? {
      kind: "draft", connection: { baseUrl: baseUrl.trim(), apiKey: apiKey.trim(),
        upstreamProtocol, responsesOptions, defaultModel: model?.trim() || null },
    } : null, [baseUrl, apiKey, upstreamProtocol, responsesOptions, model, valid]);
  return <section className="asb-provider-section" aria-label="连接测试">
    <h3>连接测试</h3>
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
