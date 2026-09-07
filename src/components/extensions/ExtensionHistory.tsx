import type { ExtensionListItem, ExtensionOperationRecord } from "../../api/client";
import { Button } from "../Button";
import { Time } from "../Time";
import { OPERATION_LABELS, outcomeText, targetLabel } from "./labels";

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
  if (records.length === 0) return null;
  const nameOf = (definitionId: string) =>
    items.find((item) => item.id === definitionId)?.name ?? definitionId;

  return (
    <section className="asb-ext-history" aria-label="操作历史">
      <h3>操作历史</h3>
      <ul className="asb-ext-history-list">
        {records.slice(0, 10).map((record) => (
          <li key={record.id} className="asb-ext-history-item">
            <div className="asb-ext-history-head">
              <span className="asb-pill-status">
                {OPERATION_LABELS[
                  record.resources.find(
                    (resource) =>
                      resource.operation === "install" || resource.operation === "remove",
                  )?.operation ?? record.resources[0]?.operation ?? "update"
                ]}
              </span>
              <span>
                {record.resources.map((resource) => nameOf(resource.definitionId)).join("、")}
              </span>
              <Time iso={record.createdAt} />
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => onRestore(record.id)}
              >
                恢复
              </Button>
            </div>
            <ul className="asb-ext-history-outcomes">
              {record.resources.flatMap((resource, resourceIndex) =>
                resource.targets.map((target, index) => (
                  <li key={`${resourceIndex}-${index}`}>
                    {targetLabel(target.target, projectNames)}：{outcomeText(target.outcome)}
                  </li>
                )),
              )}
            </ul>
            {record.rollback && (
              <p className="asb-scope-note">
                回滚：已恢复 {record.rollback.restored.length} 个目标，失败 {record.rollback.failed.length} 个。
              </p>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
