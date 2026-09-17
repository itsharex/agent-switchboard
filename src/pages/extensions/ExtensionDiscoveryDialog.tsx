import { useEffect, useState } from "react";
import { DiscoverPanel } from "../../components/extensions/DiscoverPanel";
import { AppDialog } from "../../components/AppDialog";
import { ExtensionSearch } from "./ExtensionToolbar";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

export function ExtensionDiscoveryPanel({ workspace: w }: { workspace: ExtensionWorkspace }) {
  const kind = w.nav.kind === "mcp" ? "mcp" : "skill";
  const [search, setSearch] = useState("");
  useEffect(() => setSearch(""), [kind]);
  return (
    <div className="asb-ext-discovery-panel" role="region" aria-label="从本机发现"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          w.nav.setDiscoveryOpen(false);
        }
      }}>
      <ExtensionSearch kind={kind} search={search} onSearch={setSearch} />
      <DiscoverPanel
        discovery={w.discovery}
        busy={w.writeBlocked}
        kindTab={kind}
        search={search}
        projectNames={w.projectNames}
        bindingInfo={w.bindingInfo}
        onViewDetails={(observed) => {
          if (observed.actions.managedDefinitionId)
            w.nav.openManagement(observed.actions.managedDefinitionId, observed.kind);
        }}
        onRepair={(ids) => void w.applies.repair(ids)}
      />
    </div>
  );
}

/** Kept for callers that still need a modal entry point; the main workspace
 * uses ExtensionDiscoveryPanel so discovery does not create another layer. */
export function ExtensionDiscoveryDialog({ workspace: w }: { workspace: ExtensionWorkspace }) {
  return (
    <AppDialog title="从本机发现" busy={w.busy} onClose={w.nav.closeDialog} wide>
      <ExtensionDiscoveryPanel workspace={w} />
    </AppDialog>
  );
}
