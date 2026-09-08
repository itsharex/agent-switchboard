import { Button } from "../../components/Button";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import {
  ExtensionPlanSheet,
  ExtensionRemoveSheet,
  SkillDisableScopeSheet,
} from "../../components/extensions/ExtensionPlanSheet";
import { ExtensionDetailsDialog } from "./ExtensionDetailsDialog";
import { ExtensionDiscoveryDialog } from "./ExtensionDiscoveryDialog";
import { McpEditorDialog, SkillEditorDialog } from "./ExtensionEditorDialogs";
import {
  HistoryDialog,
  NewMcpDialog,
  NewSkillDialog,
  PortableImportDialog,
  ProjectDialog,
} from "./ExtensionUtilityDialogs";
import { PortableExportSheet } from "./PortableExportSheet";
import { TakeoverConfirmSheet } from "./TakeoverConfirmSheet";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

function WorkspaceDialog({ workspace: w }: { workspace: ExtensionWorkspace }) {
  const dialog = w.nav.dialog;
  if (!dialog) return null;
  switch (dialog.type) {
    case "newMcp":
      return <NewMcpDialog workspace={w} />;
    case "newSkill":
      return <NewSkillDialog workspace={w} />;
    case "portableImport":
      return <PortableImportDialog workspace={w} />;
    case "project":
      return <ProjectDialog workspace={w} />;
    case "history":
      return <HistoryDialog workspace={w} />;
    case "import":
      return <ExtensionDiscoveryDialog workspace={w} />;
    case "export":
      return <PortableExportSheet workspace={w} item={dialog.item} />;
    case "mcpEditor":
      return <McpEditorDialog workspace={w} envelope={dialog.envelope} />;
    case "remove":
      return (
        <ExtensionRemoveSheet
          item={dialog.item}
          busy={w.busy}
          onManage={() => w.nav.showDefinition(dialog.item.id, dialog.item.kind)}
          onCancel={w.nav.closeDialog}
          onConfirm={() =>
            void w.ext.removeDefinition(dialog.item.id).then((removed) => {
              if (removed) w.nav.closeDialog();
            })
          }
        />
      );
    case "detail":
    case "skillEditor": {
      const item = w.items.find((entry) => entry.id === dialog.definitionId);
      if (!item)
        return (
          <ExtensionDialog title="读取扩展" busy={w.busy} onClose={w.nav.closeDialog}>
            <p>暂时无法读取这个扩展，请刷新扩展库。</p>
            <Button variant="secondary" onClick={() => void w.ext.runExclusive(w.ext.refresh)}>
              重新加载
            </Button>
          </ExtensionDialog>
        );
      return dialog.type === "skillEditor" && item.kind === "skill" ? (
        <SkillEditorDialog key={item.id} workspace={w} item={item} />
      ) : (
        <ExtensionDetailsDialog key={item.id} workspace={w} item={item} />
      );
    }
  }
}

export function ExtensionWorkspaceDialogs({ workspace: w }: { workspace: ExtensionWorkspace }) {
  return (
    <>
      <WorkspaceDialog workspace={w} />
      {w.plans.view && (
        <ExtensionPlanSheet
          view={w.plans.view}
          busy={w.busy}
          projectNames={w.projectNames}
          resourceNames={w.resourceNames}
          onConfirm={() => void w.plans.confirm()}
          onCancel={w.plans.close}
        />
      )}
      {w.plans.pendingDisable && (
        <SkillDisableScopeSheet
          sharedSettings={w.plans.sharedSettings}
          busy={w.busy}
          onSharedSettingsChange={w.plans.setSharedSettings}
          onConfirm={() => void w.plans.confirmScope()}
          onCancel={w.plans.cancelScope}
        />
      )}
      {w.discovery.takeover && (
        <TakeoverConfirmSheet
          takeoverPreview={w.discovery.takeover.preview}
          busy={w.busy}
          cancelTakeover={w.discovery.cancelTakeover}
          confirmTakeover={() =>
            void w.discovery.confirmTakeover().then((result) => {
              if (result) w.nav.showDefinition(result.definition.id, result.kind);
            })
          }
        />
      )}
    </>
  );
}
