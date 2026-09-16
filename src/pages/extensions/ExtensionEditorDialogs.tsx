import type { ExtensionListItem, McpEditViewEnvelope } from "../../api/client";
import type { SkillUpdatePreparation } from "../../app/extensions/extension-ops";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import { McpEditForm } from "../../components/extensions/McpEditForm";
import { SkillWorkbench } from "../../components/extensions/SkillWorkbench";
import { toast } from "../../components/use-toast";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

async function deployUpdatedDefinition(w: ExtensionWorkspace, saved: SkillUpdatePreparation | null) {
  if (!saved) return false;
  if (saved.deployment === "unverified") {
    toast({
      kind: "warning",
      title: "定义已保存，客户端部署未验证",
      description: "可从列表的“部署与诊断”操作重新部署当前版本。",
    });
    return true;
  }
  if (saved.deployment === "notRequired") return true;
  const result = await w.applies.run({
    operations: [{ operation: "update", definitionId: saved.definition.id }],
  });
  if (result.status === "cancelled") toast({
    kind: "info",
    title: "定义已保存，已取消本次客户端变更",
    description: "可从列表的“部署与诊断”操作重新部署当前版本。",
  });
  return true;
}

export function McpEditorDialog({
  workspace: w,
  envelope,
}: {
  workspace: ExtensionWorkspace;
  envelope: McpEditViewEnvelope;
}) {
  return (
    <ExtensionDialog title={`编辑 MCP · ${envelope.name}`} busy={w.busy} onClose={w.nav.closeDialog} wide>
      <McpEditForm
        envelope={envelope}
        busy={w.writeBlocked}
        onPutSecret={w.ext.putSecret}
        onCancel={w.nav.closeDialog}
        onSave={async (edit) => {
          const result = await w.ext.applyMcpEdit(envelope.id, edit);
          if (!result) return false;
          return deployUpdatedDefinition(w, result);
        }}
      />
    </ExtensionDialog>
  );
}

export function SkillEditorDialog({
  workspace: w,
  item,
}: {
  workspace: ExtensionWorkspace;
  item: Extract<ExtensionListItem, { kind: "skill" }>;
}) {
  return (
    <ExtensionDialog title={`编辑 Skill · ${item.name}`} busy={w.busy} onClose={w.nav.closeDialog} wide>
      <SkillWorkbench
        item={item}
        mcpOptions={w.items
          .filter((entry) => entry.kind === "mcp")
          .map((entry) => ({ id: entry.id, name: entry.name }))}
        busy={w.writeBlocked}
        onLoadEditor={w.ext.loadSkillEditor}
        onLoadVersions={w.ext.loadSkillVersions}
        onSaveFiles={async (id, update) => {
          const result = await w.ext.saveSkillFiles(id, update);
          return deployUpdatedDefinition(w, result);
        }}
        onRestoreVersion={async (id, digest) => {
          const result = await w.ext.restoreSkillVersion(id, digest);
          await deployUpdatedDefinition(w, result);
        }}
        onSaveDependencies={w.ext.saveSkillDependencies}
        onFork={(id) =>
          void w.ext.forkSkill(id).then((result) => {
            if (result) w.nav.openSkillEditor(result.id);
          })
        }
        onClose={w.nav.closeDialog}
      />
    </ExtensionDialog>
  );
}
