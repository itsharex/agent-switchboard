import type { FilePreview, LocalizedMessage } from "../api/client";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { PreviewInspector } from "./PreviewInspector";

interface Props {
  filePreview: FilePreview;
  busy: boolean;
  userConfigModel: string | null;
  userConfigWarnings: LocalizedMessage[];
  onConfirm: () => void;
  onCancel: () => void;
}

/** The one switch confirmation: unfolds inside the requesting row's card,
 * shows the typed write preview, and never writes on its own. Both client
 * lists mount it identically. */
export function SwitchConfirmationSection({
  filePreview,
  busy,
  userConfigModel,
  userConfigWarnings,
  onConfirm,
  onCancel,
}: Props) {
  const { t } = useI18n();
  return (
    <section className="asb-preview-inline" aria-label={t("providers.confirm.switch.title")}>
      <div className="asb-preview-inline-heading">
        <h3 className="asb-section-title">{t("providers.confirm.switch.title")}</h3>
        <div className="asb-preview-inline-actions">
          <Button variant="secondary" disabled={busy} onClick={onCancel}>
            {t("providers.confirm.switch.cancel")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={onConfirm}>
            {t("providers.confirm.switch.title")}
          </Button>
        </div>
      </div>
      <PreviewInspector filePreview={filePreview}
        userConfigModel={userConfigModel} userConfigWarnings={userConfigWarnings} />
    </section>
  );
}
