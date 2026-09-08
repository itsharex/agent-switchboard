import { useState } from "react";
import type { ExtensionListItem } from "../../api/client";
import { ExtensionDetail } from "../../components/extensions/ExtensionDetail";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import { parseTargetValue } from "../../components/extensions/labels";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

export function ExtensionDetailsDialog({
  workspace: w,
  item,
}: {
  workspace: ExtensionWorkspace;
  item: ExtensionListItem;
}) {
  const [targets, setTargets] = useState<string[]>([]);
  const updateReport = w.updates.reportMap.get(item.id) ?? null;
  return (
    <ExtensionDialog title={`扩展详情 ${item.name}`} busy={w.busy} onClose={w.nav.closeDialog} wide>
      <ExtensionDetail
        item={item}
        projects={w.ext.workspace?.projects ?? []}
        capabilities={w.ext.workspace?.capabilities ?? []}
        busy={w.writeBlocked}
        installTargets={targets}
        onInstallTargetsChange={setTargets}
        updateReport={updateReport}
        projectNames={w.projectNames}
        onInstall={() =>
          void w.plans.prepare({
            operations: [
              {
                operation: "install",
                definitionId: item.id,
                targets: targets.map(parseTargetValue).filter((target) => target !== null),
              },
            ],
          })
        }
        onChangeBinding={(binding, enable) =>
          void w.plans.prepare({
            operations: [
              {
                operation: enable ? "enable" : "disable",
                bindingId: binding.id,
              },
            ],
          })
        }
        onRemoveBinding={(binding) =>
          void w.plans.prepare({ operations: [{ operation: "remove", bindingId: binding.id }] })
        }
        onToggleLock={(binding, locked) => void w.ext.toggleBindingLock(binding.id, locked)}
        onDelete={() => w.nav.setDialog({ type: "remove", item })}
        onEditMcp={() => void w.edit(item)}
        onEditSkillContent={() => void w.edit(item)}
        onCheckUpdates={() => void w.updates.check([item.id])}
        onApplyUpdate={() => {
          if (updateReport) void w.updates.update([updateReport]);
        }}
        onDeployCurrent={() =>
          void w.plans.prepare({ operations: [{ operation: "update", definitionId: item.id }] })
        }
        onExportPortable={
          item.kind === "skill" || item.transport === "stdio"
            ? () => w.nav.setDialog({ type: "export", item })
            : undefined
        }
      />
    </ExtensionDialog>
  );
}
