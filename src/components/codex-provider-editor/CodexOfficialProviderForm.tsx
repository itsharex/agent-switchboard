import { Button } from "../Button";
import { Input } from "../Input";
import { OfficialLoginPanel } from "../OfficialLoginPanel";
import { useI18n } from "../../i18n";
import { ProviderAdvancedSettings } from "../provider-editor/ProviderAdvancedSettings";
import { ProviderNotesField } from "../provider-editor/ProviderIdentityFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import { RadioOption } from "../RadioOption";
import type { useCodexOfficialEditor } from "./useCodexOfficialEditor";

interface Props {
  formId: string;
  busy: boolean;
  editing: boolean;
  editor: ReturnType<typeof useCodexOfficialEditor>;
  onSubmit: () => void;
  onSwitchAccessMode: () => void;
}

function BasicDetails({ busy, editing, name, websiteUrl, onNameChange, onWebsiteChange, onSwitchAccessMode }: {
  busy: boolean;
  editing: boolean;
  name: string;
  websiteUrl: string;
  onNameChange: (value: string) => void;
  onWebsiteChange: (value: string) => void;
  onSwitchAccessMode: () => void;
}) {
  const { t } = useI18n();
  return <section className="asb-editor-section" aria-label={t("codex.identity.title")}>
    <h3 className="asb-section-title">{t("codex.identity.title")}</h3>
    <div className="asb-editor-section-fields">
      <div className="asb-provider-field-grid">
        <div className="asb-field"><span>{t("codex.identity.accessMode")}</span>
          {editing ? <p className="asb-provider-identity-value">{t("codex.identity.official")}</p> : <div className="asb-segments" role="radiogroup" aria-label={t("codex.identity.accessMode")}>
            <RadioOption name="codex-access-mode" checked={false} label={t("codex.identity.thirdParty")}
              disabled={busy} onChange={onSwitchAccessMode} />
            <RadioOption name="codex-access-mode" checked label={t("codex.identity.official")} disabled={busy}
              onChange={() => undefined} />
          </div>}
        </div>
        <label className="asb-field"><span>{t("codex.identity.name")}</span>
          <Input value={name} required disabled={busy} onChange={(event) => onNameChange(event.target.value)} />
        </label>
      </div>
      <div className="asb-provider-field-grid">
        <label className="asb-field"><span>{t("codex.identity.website")}</span>
          <Input type="url" value={websiteUrl} disabled={busy} placeholder={t("codex.optional")}
            onChange={(event) => onWebsiteChange(event.target.value)} />
        </label>
      </div>
    </div>
  </section>;
}

function SubscriptionSettings({ busy, value, onChange }: {
  busy: boolean;
  value: number;
  onChange: (value: number) => void;
}) {
  const { t } = useI18n();
  return <div className="asb-provider-advanced-group">
    <label className="asb-field"><span>{t("codex.official.quotaInterval")}</span>
      <Input type="number" min="0" step="1" value={String(value)} disabled={busy}
        onChange={(event) => {
          const parsed = Number(event.target.value.trim());
          onChange(Number.isSafeInteger(parsed) && parsed > 0 ? parsed : 0);
        }} />
    </label>
    <p className="asb-scope-note">{t("codex.official.quotaIntervalNote")}</p>
  </div>;
}

/** The official session owns its draft, including provider runtime parameters. */
export function CodexOfficialProviderForm({
  formId,
  busy,
  editing,
  editor,
  onSubmit,
  onSwitchAccessMode,
}: Props) {
  const { t } = useI18n();
  const { draft, setDraft, parameters } = editor;
  return <form id={formId} className="asb-provider-form" aria-label={editing ? t("codex.official.editTitle") : t("codex.official.newTitle")}
    onSubmit={(event) => { event.preventDefault(); onSubmit(); }}>
    <BasicDetails busy={busy} editing={editing} name={draft.name} websiteUrl={draft.websiteUrl ?? ""}
      onNameChange={(name) => setDraft((current) => ({ ...current, name }))}
      onWebsiteChange={(websiteUrl) => setDraft((current) => ({ ...current, websiteUrl }))}
      onSwitchAccessMode={onSwitchAccessMode} />
    <section className="asb-editor-section" aria-label={t("codex.official.title")}>
      <h3 className="asb-section-title">{t("codex.official.title")}</h3>
      <div className="asb-editor-section-fields"><OfficialLoginPanel app="codex" /></div>
    </section>
    <ProviderAdvancedSettings>
      <div className="asb-provider-advanced-action">
        <div><strong>{t("codex.editor.runtimeParams")}</strong><span>{t("codex.editor.runtimeParamsNote")}</span></div>
        <Button ref={editor.triggerRef} variant="secondary" disabled={busy || !parameters.ready}
          onClick={() => editor.setParametersOpen(true)}>{t("codex.editor.configureParams")} <span aria-hidden="true">→</span></Button>
      </div>
      <ParametersLoadStatus busy={busy} ready={parameters.ready} error={parameters.error} retry={parameters.retry} />
      <SubscriptionSettings busy={busy} value={draft.officialQuotaRefreshIntervalMinutes ?? 0}
        onChange={(value) => setDraft((current) => ({ ...current, officialQuotaRefreshIntervalMinutes: value > 0 ? value : null }))} />
      <ProviderNotesField busy={busy} value={draft.notes ?? ""}
        onChange={(notes) => setDraft((current) => ({ ...current, notes }))} />
    </ProviderAdvancedSettings>
  </form>;
}
