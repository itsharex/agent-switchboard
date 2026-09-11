import { useState } from "react";
import type { UpstreamProtocol } from "../../api/client";
import { Button } from "../Button";
import { EyeOffIcon, PreviewIcon } from "../icons";
import { Input } from "../Input";
import { PROTOCOL_AUTHENTICATION_NOTES } from "./draft";

interface Props {
  value: string;
  protocol: UpstreamProtocol | null;
  busy: boolean;
  onChange: (value: string) => void;
}

/** API key input with the reveal toggle; visibility resets on remount, so
 * consumers key it by whatever should reset the reveal state. */
export function ProviderCredentialField({ value, protocol, busy, onChange }: Props) {
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  return (
    <div className="asb-field">
      <span>API 密钥</span>
      <div className="asb-secret-control">
        <Input aria-label="API 密钥" type={apiKeyVisible ? "text" : "password"} required
          value={value} disabled={busy}
          onChange={(event) => onChange(event.target.value)} />
        <Button variant="secondary" aria-pressed={apiKeyVisible} disabled={busy}
          onClick={() => setApiKeyVisible((current) => !current)}>
          {apiKeyVisible ? <EyeOffIcon size={16} /> : <PreviewIcon size={16} />}
          {apiKeyVisible ? "隐藏密钥" : "查看密钥"}
        </Button>
      </div>
      {protocol && <p className="asb-scope-note">
        {PROTOCOL_AUTHENTICATION_NOTES[protocol]}
      </p>}
    </div>
  );
}
