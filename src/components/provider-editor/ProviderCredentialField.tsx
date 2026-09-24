import { useState } from "react";
import type { UpstreamProtocol } from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { EyeOffIcon, PreviewIcon } from "../icons";
import { Input } from "../Input";
import { PROTOCOL_AUTHENTICATION_NOTES } from "./draft";

interface Props {
  value: string;
  protocol: UpstreamProtocol | null;
  authentication?: import("../../api/shared").AuthenticationScheme | null;
  busy: boolean;
  required?: boolean;
  onChange: (value: string) => void;
}

/** API key input with the reveal toggle; visibility resets on remount, so
 * consumers key it by whatever should reset the reveal state. */
export function ProviderCredentialField({ value, protocol, authentication, busy, required = true, onChange }: Props) {
  const { t } = useI18n();
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  return (
    <div className="asb-field">
      <span>{t("providers.editor.apiKey")}</span>
      <div className="asb-secret-control">
        <Input aria-label={t("providers.editor.apiKey")} type={apiKeyVisible ? "text" : "password"} required={required}
          value={value} disabled={busy}
          onChange={(event) => onChange(event.target.value)} />
        <Button variant="secondary" aria-pressed={apiKeyVisible} disabled={busy}
          onClick={() => setApiKeyVisible((current) => !current)}>
          {apiKeyVisible ? <EyeOffIcon size={16} /> : <PreviewIcon size={16} />}
          {apiKeyVisible ? t("providers.editor.hideKey") : t("providers.editor.showKey")}
        </Button>
      </div>
      {protocol && <p className="asb-scope-note">
        {t(PROTOCOL_AUTHENTICATION_NOTES[authentication === "bearer" ? "responses"
          : authentication === "xApiKey" ? "anthropicMessages" : authentication === "xGoogApiKey" ? "geminiGenerateContent" : protocol])}
      </p>}
    </div>
  );
}
