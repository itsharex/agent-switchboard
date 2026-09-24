import { commandErrorText } from "../i18n/errors";
import { ConfirmSheet } from "../components/ConfirmSheet";
import { DiffView } from "../components/DiffView";
import { PreviewInspector } from "../components/PreviewInspector";
import { Time } from "../components/Time";
import { useI18n } from "../i18n";
import { localizedMessageText } from "../i18n/errors";
import { clientName } from "../lib/client-name";
import type { useProviders } from "./useProviders";
import type { useSwitchOperations } from "./useSwitchOperations";

interface OperationConfirmSheetsProps {
  operations: ReturnType<typeof useSwitchOperations>;
  providers: ReturnType<typeof useProviders>;
}

/** Every destructive or writing operation gets one explicit confirmation
 * sheet; nothing writes without it. The switch confirmation is not a sheet:
 * it unfolds inline inside the requesting provider row. */
export function OperationConfirmSheets({
  operations,
  providers,
}: OperationConfirmSheetsProps) {
  const { t } = useI18n();
  const { undoPending, recoverLockPending } = operations;
  return (
    <>
      {providers.pendingSave && (
        <ConfirmSheet
          title={t("operations.save.title")}
          confirmLabel={t("operations.save.title")}
          onConfirm={() => void providers.runPendingSave()}
          onCancel={() => providers.setPendingSave(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("operations.save.writesTarget", { target: providers.pendingSave.preview.preview.target })}</li>
            <li>{t("operations.save.changeCount", { count: providers.pendingSave.preview.preview.changes.length })}</li>
            {providers.pendingSave.preview.preview.warnings.map((warning) => (
              <li key={warning.key} className="asb-warn-text">{localizedMessageText(warning, t)}</li>
            ))}
            <li><PreviewInspector filePreview={providers.pendingSave.preview} userConfigModel={null} userConfigWarnings={[]} /></li>
            <li>{t("operations.save.backupDir", { dir: providers.pendingSave.preview.preview.backupDir })}</li>
          </ul>
        </ConfirmSheet>
      )}
      {providers.deletePending && (
        <ConfirmSheet
          title={t("operations.delete.title")}
          confirmLabel={t("operations.delete.confirm")}
          destructive
          onConfirm={() => void providers.runDelete()}
          onCancel={() => providers.setDeletePending(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("operations.delete.line1", { name: providers.deletePending.kind === "generic"
              ? providers.deletePending.profile.name
              : providers.deletePending.record.profile.name })}</li>
            <li>{t("operations.delete.line2")}</li>
          </ul>
        </ConfirmSheet>
      )}
      {undoPending && (
        <ConfirmSheet
          title={t("operations.undo.title")}
          confirmLabel={t("operations.undo.confirm")}
          confirmDisabled={operations.undoDiff.state !== "ready"}
          onConfirm={() => void operations.runUndo()}
          onCancel={operations.cancelUndo}
        >
          <ul className="asb-dialog-details">
            <li>{undoPending.profileName
              ? t("operations.undo.lastSwitch", { client: clientName(undoPending.app), name: undoPending.profileName })
              : t("operations.undo.lastRestore", { client: clientName(undoPending.app) })}</li>
            <li>{t("operations.undo.switchedAt")} <Time iso={undoPending.at} /></li>
            <li>{t("operations.undo.restoreNote")}</li>
            {operations.undoDiff.state === "loading" ? (
              <li>{t("operations.undo.diffLoading")}</li>
            ) : operations.undoDiff.state === "error" ? (
              <li className="asb-warn-text">{commandErrorText(operations.undoDiff.error, t)}</li>
            ) : operations.undoDiff.state === "ready" && operations.undoDiff.changes.length === 0 ? (
              <li>{t("operations.undo.diffEmpty")}</li>
            ) : operations.undoDiff.state === "ready" ? (
              <li><DiffView changes={operations.undoDiff.changes.map((change) => ({
                ...change,
                before: change.after,
                after: change.before,
              }))} label={t("operations.undo.diffLabel")} /></li>
            ) : null}
          </ul>
        </ConfirmSheet>
      )}
      {recoverLockPending && (
        <ConfirmSheet
          title={t("operations.recoverLock.title")}
          confirmLabel={t("operations.recoverLock.confirm")}
          destructive
          onConfirm={() => void operations.runRecoverStaleLock()}
          onCancel={() => operations.setRecoverLockPending(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("operations.recoverLock.line1", { client: clientName(recoverLockPending) })}</li>
            <li>{t("operations.recoverLock.line2")}</li>
          </ul>
        </ConfirmSheet>
      )}
    </>
  );
}
