import type { ReactNode } from "react";
import type { ExtensionsDeps } from "../app/extensions/extension-ops";
import { pickDirectory, pickSkillZip } from "../api/client";
import { EXTENSION_SECTIONS, type ExtensionSection } from "../app/navigation";
import { Button } from "../components/Button";
import { SkillSourceBrowser } from "../components/extensions/SkillSourceBrowser";
import { ExtensionLibraryPanel } from "./extensions/ExtensionLibraryPanel";
import { ExtensionToolbar } from "./extensions/ExtensionToolbar";
import { ExtensionDiscoveryPanel } from "./extensions/ExtensionDiscoveryDialog";
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

export function ExtensionsPage(props: ExtensionsPageProps) {
  const workspace = useExtensionWorkspace(props);
  const recovery = workspace.ext.workspace?.recoveryRequired ?? [];
  const section = workspace.nav.section;
  const content = workspace.nav.discoveryOpen ? (
    <ExtensionDiscoveryPanel workspace={workspace} />
  ) : workspace.nav.sourceBrowser && workspace.nav.kind === "skill" ? (
    <SkillSourceBrowser
      busy={workspace.writeBlocked} items={workspace.items} onScanLocal={workspace.ext.scanLocal}
      onImport={workspace.importCandidate} onPickDirectory={pickDirectory} onPickZip={pickSkillZip} />
  ) : (
    <ExtensionLibraryPanel workspace={workspace} />
  );
  return (
    <section
      className="asb-ext"
      aria-label="扩展"
      data-view={workspace.nav.discoveryOpen ? "discovery" : workspace.nav.sourceBrowser ? "sources" : section}
    >
      <ExtensionToolbar workspace={workspace} />
      {recovery.length > 0 && (
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
      )}
      {EXTENSION_SECTIONS.map(({ value }) => (
        <ExtensionContentPanel key={value} section={value} active={section}>
          {section === value ? content : null}
        </ExtensionContentPanel>
      ))}
      <ExtensionWorkspaceDialogs workspace={workspace} />
    </section>
  );
}
