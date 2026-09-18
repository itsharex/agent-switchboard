import type { ReactNode } from "react";
import type { ExtensionsDeps } from "../app/extensions/extension-ops";
import { pickDirectory, pickFile } from "../api/client";
import { EXTENSION_SECTIONS, type ExtensionSection } from "../app/navigation";
import { Button } from "../components/Button";
import { SkillSourceBrowser } from "../components/extensions/SkillSourceBrowser";
import { ExtensionLibraryPanel } from "./extensions/ExtensionLibraryPanel";
import { ExtensionToolbar } from "./extensions/ExtensionToolbar";
import { ExtensionDiscoveryView } from "./extensions/ExtensionDiscoveryView";
import { ExtensionWorkspaceDialogs } from "./extensions/ExtensionWorkspaceDialogs";
import type { ExtensionNavigation } from "./extensions/useExtensionView";
import { useExtensionWorkspace } from "./extensions/useExtensionWorkspace";

type ExtensionsPageProps = ExtensionsDeps & ExtensionNavigation;

function ExtensionContentPanel({ section, active, children }: {
  section: ExtensionSection;
  active: ExtensionSection;
  children: ReactNode;
}) {
  return (
    <div id={`ext-workspace-${section}-panel`} role="tabpanel"
      aria-labelledby={`ext-workspace-${section}-tab`} hidden={section !== active}>
      {children}
    </div>
  );
}

/** Discovery and source browsing are full-page editor frames that replace the
 * whole workspace, mirroring the provider editor's mounting pattern; only the
 * library keeps the toolbar-and-tabs workspace. */
export function ExtensionsPage(props: ExtensionsPageProps) {
  const workspace = useExtensionWorkspace(props);
  const recovery = workspace.ext.workspace?.recoveryRequired ?? [];
  const section = workspace.nav.section;
  const dialogs = <ExtensionWorkspaceDialogs workspace={workspace} />;
  const recoveryBanner = recovery.length > 0 ? (
    <div className="asb-banner asb-banner-error" role="alert" aria-label="扩展恢复告警">
      <span>存在未能自动恢复的扩展操作，扩展写入已暂停：{recovery.join("；")}</span>
      <Button
        variant="secondary"
        disabled={workspace.busy}
        onClick={() => void workspace.ext.recoverTransactions()}
      >
        尝试恢复
      </Button>
    </div>
  ) : null;
  if (workspace.nav.discoveryOpen) {
    return (
      <div className="asb-editor-route">
        <ExtensionDiscoveryView workspace={workspace} notice={recoveryBanner} />
        {dialogs}
      </div>
    );
  }
  if (workspace.nav.sourceBrowser && workspace.nav.kind === "skill") {
    return (
      <div className="asb-editor-route">
        <SkillSourceBrowser
          busy={workspace.writeBlocked} items={workspace.items} onScanLocal={workspace.ext.scanLocal}
          onImport={workspace.importCandidate} onPickDirectory={pickDirectory}
          onPickZip={() => pickFile("ZIP archives", ["zip"])}
          onBack={() => workspace.nav.setSourceBrowser(false)} notice={recoveryBanner} />
        {dialogs}
      </div>
    );
  }
  return (
    <section
      className="asb-ext"
      aria-label="扩展"
      data-view={section}
    >
      <ExtensionToolbar workspace={workspace} />
      {recoveryBanner}
      {EXTENSION_SECTIONS.map(({ value }) => (
        <ExtensionContentPanel key={value} section={value} active={section}>
          {section === value ? (
            <ExtensionLibraryPanel workspace={workspace} />
          ) : null}
        </ExtensionContentPanel>
      ))}
      {dialogs}
    </section>
  );
}
