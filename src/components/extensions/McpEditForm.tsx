import { Save, X } from "lucide-react";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Input } from "../Input";
import { McpClientFields, McpJsonField } from "./mcp-create/McpFormFields";
import { McpMetadataFields } from "./mcp-create/McpMetadataFields";
import { McpWizard } from "./mcp-create/McpWizard";
import { useMcpEditForm, type McpEditFormProps } from "./mcp-edit/useMcpEditForm";

export type { McpEditFormProps } from "./mcp-edit/useMcpEditForm";

export function McpEditForm(props: McpEditFormProps) {
  return <McpEditor key={props.envelope.id + ":" + props.envelope.revision} {...props} />;
}

function McpEditor(props: McpEditFormProps) {
  const { t } = useI18n();
  const form = useMcpEditForm(props);
  if (form.wizard) return <McpWizard initial={form.wizard} original={form.original} busy={form.busy}
    onApply={form.applyWizard} onCancel={form.closeWizard} />;
  return <form className="asb-form asb-mcp-create" aria-label={t("mcp.edit.aria")} aria-busy={form.busy} noValidate
    onSubmit={(event) => { event.preventDefault(); void form.submit(); }}>
    <label className="asb-field"><span>{t("mcp.edit.name")}</span>
      <Input code aria-label={t("mcp.edit.name")} value={form.name} disabled={form.busy}
        onChange={(event) => form.changeName(event.target.value)} />
    </label>
    <McpMetadataFields value={form.metadata} busy={form.busy} onChange={form.changeMetadata} />
    {props.initialClients && <McpClientFields clients={form.clients} busy={form.busy} onToggle={form.toggleClient} />}
    <McpJsonField value={form.json} busy={form.busy} focus={form.focusJson} invalid={Boolean(form.error)}
      onChange={form.changeJson} onWizard={form.openWizard} />
    {form.error && <p className="asb-warn-text asb-mcp-error" role="alert">{form.error}</p>}
    <div className="asb-mcp-actions">
      <Button variant="secondary" disabled={form.busy} onClick={props.onCancel}><X size={16} aria-hidden="true" />{t("confirm.cancel")}</Button>
      <Button type="submit" variant="primary" disabled={form.busy}>
        <Save size={16} aria-hidden="true" />{form.saving ? t("mcp.edit.saving") : t("mcp.edit.save")}
      </Button>
    </div>
  </form>;
}
