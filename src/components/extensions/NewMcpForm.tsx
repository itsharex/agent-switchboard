import { Save } from "lucide-react";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { McpClientFields, McpIdentityFields, McpJsonField } from "./mcp-create/McpFormFields";
import { McpWizard } from "./mcp-create/McpWizard";
import { McpMetadataFields } from "./mcp-create/McpMetadataFields";
import { useMcpForm, type NewMcpFormProps } from "./mcp-create/useMcpForm";

export function NewMcpForm(props: NewMcpFormProps) {
  const { t } = useI18n();
  const form = useMcpForm(props);
  if (form.wizard) {
    return <McpWizard initial={form.wizard} busy={form.busy}
      onApply={form.applyWizard} onCancel={form.closeWizard} />;
  }
  return (
    <form className="asb-form asb-mcp-create" aria-label={t("mcp.form.newAria")} aria-busy={form.busy} noValidate
      onSubmit={(event) => { event.preventDefault(); void form.submit(); }}>
      <McpIdentityFields name={form.name} preset={form.preset} busy={form.busy}
        onNameChange={form.changeName} onPresetChange={form.selectPreset} />
      <McpMetadataFields value={form.metadata} busy={form.busy} onChange={form.changeMetadata} />
      <McpClientFields clients={form.clients} busy={form.busy} onToggle={form.toggleClient} />
      <McpJsonField value={form.json} busy={form.busy} focus={form.focusJson} invalid={Boolean(form.error)}
        onChange={form.changeJson} onWizard={form.openWizard} />
      {form.error && <p className="asb-warn-text asb-mcp-error" role="alert">{form.error}</p>}
      <div className="asb-mcp-actions">
        <Button type="submit" variant="primary" disabled={form.busy}>
          <Save size={16} aria-hidden="true" />
          {form.saving ? t("mcp.action.saving") : form.clients.length ? t("mcp.action.saveAndEnable") : t("mcp.action.saveOnly")}
        </Button>
      </div>
    </form>
  );
}
