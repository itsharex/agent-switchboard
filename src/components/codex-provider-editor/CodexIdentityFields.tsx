import type { AppKind } from "../../api/client";
import { useI18n } from "../../i18n";
import { ClientLogo } from "../ClientLogo";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import type { CodexEditorDraft } from "./draft";
import type { SetCodexDraft } from "./useCodexProviderEditor";

interface IdentityProps {
  draft: CodexEditorDraft;
  busy: boolean;
  editing: boolean;
  setDraft: SetCodexDraft;
  /** Switching clients starts the other client's editor instead of mutating
   * this draft. */
  onSwitchClient: (app: AppKind) => void;
}

interface AccessModeProps {
  busy: boolean;
  onSwitchAccessMode: (official: boolean) => void;
}

export function CodexAccessMode({ busy, onSwitchAccessMode }: AccessModeProps) {
  const { t } = useI18n();
  return (
    <section className="asb-editor-section" aria-label={t("codex.identity.accessMode")}>
      <h3 className="asb-section-title">{t("codex.identity.accessMode")}</h3>
      <div className="asb-editor-section-fields">
        <div className="asb-field">
          <span>{t("codex.identity.chooseConnectionType")}</span>
          <div className="asb-segments" role="radiogroup" aria-label={t("codex.identity.accessMode")}>
            <RadioOption name="codex-access-mode" checked label={t("codex.identity.thirdParty")}
              disabled={busy} onChange={() => onSwitchAccessMode(false)} />
            <RadioOption name="codex-access-mode" checked={false} label={t("codex.identity.official")}
              disabled={busy} onChange={() => onSwitchAccessMode(true)} />
          </div>
        </div>
      </div>
    </section>
  );
}

function ClientField({ busy, editing, onSwitchClient }: IdentityProps) {
  const { t } = useI18n();
  if (editing) {
    return <>
      <div className="asb-field"><span>{t("codex.identity.client")}</span>
        <p className="asb-provider-identity-value"><ClientLogo app="codex" className="asb-edit-logo" />Codex</p>
      </div>
      <div className="asb-field"><span>{t("codex.identity.accessMode")}</span>
        <p className="asb-provider-identity-value">{t("codex.identity.thirdParty")}</p>
      </div>
    </>;
  }
  return <label className="asb-field"><span>{t("codex.identity.client")}</span>
    <div className="asb-client-control">
      <ClientLogo app="codex" className="asb-edit-logo" />
      <Select ariaLabel={t("codex.identity.client")} value="codex" disabled={busy}
        options={[{ value: "codex", label: "Codex" }, { value: "claude", label: "Claude" }]}
        onChange={(app) => { if (app !== "codex") onSwitchClient(app as AppKind); }} />
    </div>
  </label>;
}

export function CodexIdentityFields(props: IdentityProps) {
  const { t } = useI18n();
  const { draft, busy, setDraft } = props;
  return (
    <section className="asb-editor-section" aria-label={t("codex.identity.title")}>
      <h3 className="asb-section-title">{t("codex.identity.title")}</h3>
      <div className="asb-editor-section-fields">
        <div className="asb-provider-field-grid"><ClientField {...props} /></div>
        <div className="asb-provider-field-grid">
          <label className="asb-field"><span>{t("codex.identity.name")}</span>
            <Input value={draft.name} required disabled={busy}
              onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))} />
          </label>
          <label className="asb-field"><span>{t("codex.identity.website")}</span>
            <Input type="url" value={draft.websiteUrl} disabled={busy} placeholder={t("codex.optional")}
              onChange={(event) => setDraft((current) => ({ ...current, websiteUrl: event.target.value }))} />
          </label>
        </div>
      </div>
    </section>
  );
}
