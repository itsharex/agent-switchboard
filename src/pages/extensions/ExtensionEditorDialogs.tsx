import type { ExtensionListItem, McpEditViewEnvelope } from "../../api/client";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import { McpEditForm } from "../../components/extensions/McpEditForm";
import { SkillWorkbench } from "../../components/extensions/SkillWorkbench";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

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
          if (result) {
            w.nav.showDefinition(result.definition.id, "mcp");
            if (result.plan) w.plans.setView(result.plan);
          }
          return result !== null;
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
          if (result?.plan) w.plans.setView(result.plan);
          return result;
        }}
        onRestoreVersion={(id, digest) =>
          void w.ext.restoreSkillVersion(id, digest).then((result) => {
            if (result?.plan) w.plans.setView(result.plan);
          })
        }
        onSaveDependencies={async (id, update) => {
          await w.ext.saveSkillDependencies(id, update);
        }}
        onFork={(id) =>
          void w.ext.forkSkill(id).then((result) => {
            if (result) w.nav.showDefinition(result.id, "skill", true);
          })
        }
        onClose={() => w.nav.showDefinition(item.id, "skill")}
      />
    </ExtensionDialog>
  );
}
