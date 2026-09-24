import { useState } from "react";
import type { AppKind } from "../../api/client";
import { useI18n } from "../../i18n";
import { clientName } from "../../lib/client-name";
import { ClientLogo } from "../ClientLogo";
import { ChevronDownIcon } from "../icons";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import { Textarea } from "../Textarea";
import { defaultConnection } from "./draft";
import type { ProviderEditorState } from "./useProviderEditor";

interface IdentityProps {
  editor: ProviderEditorState;
  busy: boolean;
  editing: boolean;
  /** Switching to Codex replaces the editor session; a Claude draft never
   * mutates into a Codex draft. */
  onSwitchClient: (app: AppKind) => void;
}

interface AccessModeProps {
  editor: ProviderEditorState;
  busy: boolean;
  officialTakenApps: AppKind[];
  onOpenOfficial: (app: AppKind) => void;
}

export function ProviderAccessMode({ editor, busy, officialTakenApps, onOpenOfficial }: AccessModeProps) {
  const { t } = useI18n();
  const { draft, setDraft, setLoginDone } = editor;
  return (
    <section className="asb-editor-section" aria-label={t("providers.label.accessMode")}>
      <h3 className="asb-section-title">{t("providers.label.accessMode")}</h3>
      <div className="asb-editor-section-fields">
        <div className="asb-field">
          <span>{t("providers.editor.chooseConnectionType")}</span>
          <div className="asb-segments" role="radiogroup" aria-label={t("providers.label.accessMode")}>
            <RadioOption name="access-mode" checked={draft.routeMode === "custom"}
              disabled={busy} label={t("providers.editor.access.custom")} onChange={() => {
                setDraft((current) => ({ ...current, routeMode: "custom", ...defaultConnection(current.app) }));
                setLoginDone(false);
              }} />
            <RadioOption name="access-mode" checked={draft.routeMode === "official"}
              disabled={busy} label={t("providers.label.officialLogin")} onChange={() => {
                if (officialTakenApps.includes(draft.app)) { onOpenOfficial(draft.app); return; }
                setDraft((current) => ({
                  ...current,
                  routeMode: "official",
                  name: current.name.trim() || t("providers.editor.officialDefaultName", { client: clientName(current.app) }),
                  model: null,
                  baseUrl: null,
                  apiKey: "",
                  upstreamProtocol: null,
                  authentication: null,
                  responsesOptions: null,
                  maxOutputTokens: null,
                  modelOptions: null,
                  usageQuery: null,
                  officialQuotaRefreshIntervalMinutes: null,
                }));
                setLoginDone(false);
              }} />
          </div>
        </div>
      </div>
    </section>
  );
}

function ClientField({ editor, busy, editing, onSwitchClient }: IdentityProps) {
  const { t } = useI18n();
  const { draft } = editor;
  if (editing) {
    return <>
      <div className="asb-field"><span>{t("providers.editor.client")}</span>
        <p className="asb-provider-identity-value"><ClientLogo app={draft.app} className="asb-edit-logo" />{clientName(draft.app)}</p>
      </div>
      <div className="asb-field"><span>{t("providers.label.accessMode")}</span>
        <p className="asb-provider-identity-value">{draft.routeMode === "official" ? t("providers.label.officialLogin") : t("providers.editor.access.custom")}</p>
      </div>
    </>;
  }
  return <label className="asb-field"><span>{t("providers.editor.client")}</span>
    <div className="asb-client-control">
      <ClientLogo app={draft.app} className="asb-edit-logo" />
      <Select ariaLabel={t("providers.editor.client")} value={draft.app} disabled={busy}
        options={[{ value: "codex", label: "Codex" }, { value: "claude", label: "Claude" }]}
        onChange={(app) => { if (app !== draft.app) onSwitchClient(app as AppKind); }} />
    </div>
  </label>;
}

export function ProviderIdentityFields(props: IdentityProps) {
  const { t } = useI18n();
  const { editor, busy } = props;
  const { draft, setDraft } = editor;
  return (
    <section className="asb-editor-section" aria-label={t("providers.editor.section.identity")}>
      <h3 className="asb-section-title">{t("providers.editor.section.identity")}</h3>
      <div className="asb-editor-section-fields">
        <div className="asb-provider-field-grid"><ClientField {...props} /></div>
        <div className="asb-provider-field-grid">
          <label className="asb-field"><span>{t("providers.editor.name")}</span>
            <Input value={draft.name} required disabled={busy}
              onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))} />
          </label>
          <label className="asb-field"><span>{t("providers.editor.website")}</span>
            <Input type="url" value={draft.websiteUrl ?? ""} disabled={busy} placeholder={t("providers.editor.optional")}
              onChange={(event) => setDraft((current) => ({ ...current, websiteUrl: event.target.value }))} />
          </label>
        </div>
      </div>
    </section>
  );
}

export function ProviderNotesField({ value, busy, onChange }: {
  value: string | null;
  busy: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(() => Boolean(value?.trim()));
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>{t("providers.editor.notes")}</span><span className="asb-provider-disclosure-value">{value?.trim() ? t("providers.editor.notes.filled") : t("providers.editor.notes.optional")}</span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body">
        <Textarea aria-label={t("providers.editor.notes")} rows={3} value={value ?? ""} disabled={busy}
          placeholder={t("providers.editor.notesPlaceholder")}
          onChange={(event) => onChange(event.target.value)} />
      </div>
    </details>
  );
}
