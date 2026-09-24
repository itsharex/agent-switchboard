import { useI18n } from "../../i18n";
import { localizedMessageText } from "../../i18n/errors";
import { Button } from "../Button";
import { DiffView } from "../DiffView";
import { PreviewInspector } from "../PreviewInspector";
import type { ProviderRepairState } from "./use-provider-repair";

export function ProviderRepairPane({ repair, canRepair, disabled }: {
  repair: ProviderRepairState; canRepair: boolean; disabled: boolean;
}) {
  const { t } = useI18n();
  const pending = repair.pending;
  return <section className="asb-doctor-repair" aria-label={t("doctor.repair")}>
    <p className="asb-scope-note">{t("doctor.repairNote")}</p>
    {repair.status && <p role="status">{repair.status}</p>}
    {repair.error && <p className="asb-warn-text" role="alert">{repair.error}</p>}
    {repair.warnings.length > 0 && <ul className="asb-warnings">{repair.warnings.map((warning, index) =>
      <li key={index}>{localizedMessageText(warning, t)}</li>)}</ul>}
    {repair.receipt && <p className="asb-scope-note">{t("doctor.backup", { id: repair.receipt.outcome.backup.id })}
      <br />{t("doctor.backupLocation")}</p>}
    {repair.receipt && !repair.receipt.canUndo && <p className="asb-scope-note">{t("doctor.catalogOnly")}</p>}
    {pending?.kind === "repair" && <PreviewInspector filePreview={pending.preview.file} userConfigModel={null} userConfigWarnings={[]} />}
    {pending?.kind === "undo" && <DiffView changes={pending.preview.changes} label={t("doctor.undo")} />}
    <div className="asb-panel-actions">
      {pending ? <>
        <Button variant="secondary" disabled={disabled} onClick={() => void repair.cancel()}>{t("doctor.cancelPreview")}</Button>
        <Button variant="primary" disabled={disabled} onClick={() => void repair.confirm()}>
          {t(pending.kind === "repair" ? "doctor.confirmRepair" : "doctor.confirmUndo")}</Button>
      </> : <>
        <Button variant="secondary" disabled={disabled || !canRepair}
          onClick={() => void repair.preview("repair")}>{t("doctor.repair")}</Button>
        {repair.receipt?.canUndo && <Button variant="secondary" disabled={disabled}
          onClick={() => void repair.preview("undo")}>{t("doctor.undo")}</Button>}
      </>}
    </div>
  </section>;
}
