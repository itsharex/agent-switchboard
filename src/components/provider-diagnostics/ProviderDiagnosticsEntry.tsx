import { useState } from "react";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { ProviderDiagnosticsDialog } from "./ProviderDiagnosticsDialog";

export function ProviderDiagnosticsEntry({ profileId, name, active, disabled }: {
  profileId: string; name: string; active: boolean; disabled: boolean;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  return <section className="asb-editor-section asb-provider-diagnostics-entry" aria-label={t("doctor.title")}>
    <h3 className="asb-section-title">{t("doctor.title")}</h3>
    <p className="asb-field-help">{t("doctor.savedOnly")}</p>
    <Button variant="secondary" disabled={disabled} onClick={() => setOpen(true)}>{t("doctor.open")}</Button>
    {open && active && <ProviderDiagnosticsDialog profileId={profileId} name={name} onClose={() => setOpen(false)} />}
  </section>;
}
