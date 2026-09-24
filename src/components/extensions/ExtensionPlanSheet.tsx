import type {
  ExtensionListItem,
  ExtensionPlanView,
  KeyChange,
  PlanChangeView,
  PlannedTargetView,
} from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { DiffView } from "../DiffView";
import { Select } from "../Select";
import { AppDialog } from "../AppDialog";
import { OPERATION_LABELS, targetLabel } from "./labels";

interface Props {
  view: ExtensionPlanView;
  busy: boolean;
  projectNames: ReadonlyMap<string, string>;
  resourceNames: ReadonlyMap<string, string>;
  onConfirm: () => void;
  onCancel: () => void;
}

function toKeyChange(change: PlanChangeView): KeyChange {
  return {
    key: change.pointer,
    kind: change.after == null ? "remove" : "set",
    before: change.before ?? null,
    after: change.after ?? null,
  };
}

function TargetPreview({ target, label }: { target: PlannedTargetView; label: string }) {
  const { t } = useI18n();
  return (
    <div className="asb-ext-plan-target">
      <h4 className="asb-section-title">{label}</h4>
      {target.warnings.map((warning) => (
        <p key={warning} className="asb-warn-text">
          {t("extensions.plan.warning", { warning })}
        </p>
      ))}
      {target.changes.length > 0 && (
        <DiffView changes={target.changes.map(toKeyChange)} label={t("extensions.plan.diffLabel", { target: label })} />
      )}
      {target.files && target.files.length > 0 && (
        <div>
          <p className="asb-scope-note">
            {target.files.some((file) => file.action === "remove")
              ? t("extensions.plan.filesRemove")
              : t("extensions.plan.filesDeploy")}
          </p>
          <ul className="asb-ext-file-list">
            {target.files.map((file) => (
              <li key={file.relativePath} className="asb-code">
                {file.relativePath}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** The only resident confirmation for extension writes: shown when a
 * prepared plan would write sensitive connection data. Every other write
 * applies immediately through the shared pipeline. */
export function ExtensionPlanSheet({ view, busy, projectNames, resourceNames, onConfirm, onCancel }: Props) {
  const { t } = useI18n();
  const operation =
    view.operations.find((entry) => entry.operation === "install" || entry.operation === "remove")
      ?.operation ??
    view.operations[0]?.operation ??
    "update";
  return (
    <AppDialog
      title={t("extensions.plan.confirmTitle", { operation: t(OPERATION_LABELS[operation]) })}
      busy={busy}
      onClose={onCancel}
      wide
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            {t("confirm.cancel")}
          </Button>
          <Button
            variant={operation === "remove" || operation === "restore" ? "danger" : "primary"}
            disabled={busy}
            onClick={onConfirm}
          >
            {t("extensions.plan.confirmWrite")}
          </Button>
        </>
      }
    >
      <p className="asb-scope-note">
        {t("extensions.plan.note")}
        {view.operations.length > 1 && ` ${t("extensions.plan.multiNote")}`}
      </p>
      {view.operations.map((entry, index) => (
        <section key={index} className="asb-ext-section">
          <h3 className="asb-section-title">
            {resourceNames.get(entry.definitionId) ?? entry.definitionId}
            <span className="asb-scope-note"> · {t(OPERATION_LABELS[entry.operation])}</span>
          </h3>
          {entry.targets.map((target, targetIndex) => (
            <TargetPreview
              key={targetIndex}
              target={target}
              label={targetLabel(target.target, projectNames)}
            />
          ))}
        </section>
      ))}
    </AppDialog>
  );
}

/** One confirmation for the whole deletion: bindings are removed first (each
 * restorable from the transaction history), then the definition is deleted. */
export function ExtensionRemoveSheet({
  item,
  busy,
  onConfirm,
  onCancel,
}: {
  item: ExtensionListItem;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const bound = item.bindings.length > 0;
  return (
    <AppDialog
      title={t("extensions.remove.title")}
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            {t("confirm.cancel")}
          </Button>
          <Button variant="danger" disabled={busy} onClick={onConfirm}>
            {t(bound ? "extensions.remove.confirmUnbind" : "extensions.remove.confirm")}
          </Button>
        </>
      }
    >
      <p>{t("extensions.remove.body", { name: item.name })}</p>
      {bound ? (
        <p>{t("extensions.remove.boundBody", { count: item.bindings.length })}</p>
      ) : (
        <p>{t("extensions.remove.unboundBody")}</p>
      )}
    </AppDialog>
  );
}

export function SkillDisableScopeSheet({
  sharedSettings,
  busy,
  onSharedSettingsChange,
  onConfirm,
  onCancel,
}: {
  sharedSettings: boolean | null;
  busy: boolean;
  onSharedSettingsChange: (shared: boolean) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  return (
    <AppDialog
      title={t("extensions.disableScope.title")}
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            {t("confirm.cancel")}
          </Button>
          <Button variant="primary" disabled={busy || sharedSettings === null} onClick={onConfirm}>
            {t("extensions.disableScope.confirm")}
          </Button>
        </>
      }
    >
      <p>{t("extensions.disableScope.body")}</p>
      <label className="asb-field">
        <span>{t("extensions.disableScope.field")}</span>
        <Select
          value={sharedSettings === null ? null : sharedSettings ? "shared" : "local"}
          options={[
            { value: "local", label: t("extensions.disableScope.local") },
            { value: "shared", label: t("extensions.disableScope.shared") },
          ]}
          onChange={(value) => onSharedSettingsChange(value === "shared")}
          placeholder={t("extensions.disableScope.placeholder")}
          ariaLabel={t("extensions.disableScope.selectAria")}
          disabled={busy}
        />
      </label>
    </AppDialog>
  );
}
