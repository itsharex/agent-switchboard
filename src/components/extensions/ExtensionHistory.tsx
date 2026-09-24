import type { ExtensionListItem, ExtensionOperationRecord } from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Time } from "../Time";
import { OPERATION_LABELS, outcomeText, targetLabel } from "./labels";
import { UpdateIcon } from "../icons";

interface Props {
  records: ExtensionOperationRecord[];
  items: ExtensionListItem[];
  busy: boolean;
  projectNames: ReadonlyMap<string, string>;
  onRestore: (operationId: string) => void;
}

/** The most recent finished batch operations with their per-resource,
 * per-target outcomes; each one can be re-planned as a restore through the
 * standard preview flow. */
export function ExtensionHistory({ records, items, busy, projectNames, onRestore }: Props) {
  const { t } = useI18n();
  if (records.length === 0)
    return (
      <div className="asb-empty-state">
        <span className="asb-empty-state-icon" aria-hidden="true">
          <UpdateIcon />
        </span>
        <h3 className="asb-section-title">{t("extensions.history.empty")}</h3>
      </div>
    );
  const nameOf = (definitionId: string) =>
    items.find((item) => item.id === definitionId)?.name ?? definitionId;

  return (
    <section className="asb-ext-history" aria-label={t("extensions.history.title")}>
      <ul className="asb-ext-history-list">
        {records.slice(0, 10).map((record) => (
          <li key={record.id} className="asb-ext-history-item">
            <div className="asb-ext-history-head">
              <span className="asb-pill-status">
                {
                  t(
                    OPERATION_LABELS[
                      record.resources.find(
                        (resource) => resource.operation === "install" || resource.operation === "remove",
                      )?.operation ??
                        record.resources[0]?.operation ??
                        "update"
                    ],
                  )
                }
              </span>
              <span>{record.resources.map((resource) => nameOf(resource.definitionId)).join(t("extensions.join.comma"))}</span>
              <Time iso={record.createdAt} />
              <Button variant="secondary" disabled={busy} onClick={() => onRestore(record.id)}>
                {t("extensions.history.restore")}
              </Button>
            </div>
            <ul className="asb-ext-history-outcomes">
              {record.resources.flatMap((resource, resourceIndex) =>
                resource.targets.map((target, index) => (
                  <li key={`${resourceIndex}-${index}`}>
                    {t("extensions.history.outcomeLine", {
                      target: targetLabel(target.target, projectNames),
                      outcome: outcomeText(target.outcome),
                    })}
                  </li>
                )),
              )}
            </ul>
            {record.rollback && (
              <p className="asb-scope-note">
                {t("extensions.history.rollback", {
                  restored: record.rollback.restored.length,
                  failed: record.rollback.failed.length,
                })}
              </p>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
