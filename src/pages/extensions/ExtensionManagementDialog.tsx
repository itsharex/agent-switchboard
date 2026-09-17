import { useState } from "react";
import type { ExtensionListItem } from "../../api/client";
import { ExtensionManagement } from "../../components/extensions/ExtensionManagement";
import { AppDialog } from "../../components/AppDialog";
import { parseTargetValue } from "../../components/extensions/labels";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

export function ExtensionManagementDialog({
  workspace: w,
  item,
}: {
  workspace: ExtensionWorkspace;
  item: ExtensionListItem;
}) {
  const [targets, setTargets] = useState<string[]>([]);
  const updateReport = w.updates.reportMap.get(item.id) ?? null;
  return (
    <AppDialog title={`部署与诊断 · ${item.name}`} busy={w.busy} onClose={w.nav.closeDialog} wide>
      <ExtensionManagement
        item={item}
        projects={w.ext.workspace?.projects ?? []}
        capabilities={w.ext.workspace?.capabilities ?? []}
        busy={w.writeBlocked}
        installTargets={targets}
        onInstallTargetsChange={setTargets}
        updateReport={updateReport}
        projectNames={w.projectNames}
        onInstall={() =>
          void w.applies.run({
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
          void w.applies.run({
            operations: [
              {
                operation: enable ? "enable" : "disable",
                bindingId: binding.id,
              },
            ],
          })
        }
        onRemoveBinding={(binding) =>
          void w.applies.run({ operations: [{ operation: "remove", bindingId: binding.id }] })
        }
        onToggleLock={(binding, locked) => void w.ext.toggleBindingLock(binding.id, locked)}
        onDelete={() => w.nav.setDialog({ type: "remove", item })}
        onCheckUpdates={() => void w.updates.check([item.id])}
        onApplyUpdate={() => {
          if (updateReport) void w.updates.update([updateReport]);
        }}
        onDeployCurrent={() =>
          void w.applies.run({ operations: [{ operation: "update", definitionId: item.id }] })
        }
        onExportPortable={
          item.kind === "skill" || item.transport === "stdio"
            ? () => w.nav.setDialog({ type: "export", item })
            : undefined
        }
      />
    </AppDialog>
  );
}
