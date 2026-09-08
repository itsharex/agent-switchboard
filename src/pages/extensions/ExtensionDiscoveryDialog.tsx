import { EXTENSION_SECTIONS } from "../../app/navigation";
import { DiscoverPanel } from "../../components/extensions/DiscoverPanel";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import { Tabs } from "../../components/Tabs";
import { ExtensionFilters } from "./ExtensionToolbar";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

type DiscoverySection = Exclude<(typeof EXTENSION_SECTIONS)[number], { value: "instructions" }>;

const DISCOVERY_SECTIONS = EXTENSION_SECTIONS.filter(
  (section): section is DiscoverySection => section.value !== "instructions",
);

export function ExtensionDiscoveryDialog({ workspace: w }: { workspace: ExtensionWorkspace }) {
  if (w.nav.kind === null) return null;
  return (
    <ExtensionDialog title="从本机发现" busy={w.busy} onClose={w.nav.closeDialog} wide>
      <Tabs value={w.nav.kind} onChange={w.nav.changeSection} scope="ext-discovery"
        tabs={DISCOVERY_SECTIONS.map((tab) => ({
          ...tab,
          controls: `ext-discovery-${tab.value}-panel`,
        }))}
        label="扩展类型" />
      {DISCOVERY_SECTIONS.map((section) => (
        <div key={section.value} id={`ext-discovery-${section.value}-panel`} role="tabpanel"
          aria-labelledby={`ext-discovery-${section.value}-tab`} hidden={w.nav.kind !== section.value}>
          {w.nav.kind === section.value && <>
            <ExtensionFilters workspace={w} />
            <DiscoverPanel
              discovery={w.discovery}
              busy={w.writeBlocked}
              kindTab={section.value}
              clientFilter={w.nav.client}
              search={w.nav.search}
              projectNames={w.projectNames}
              bindingInfo={w.bindingInfo}
              onViewDetails={(observed) => {
                if (observed.actions.managedDefinitionId)
                  w.nav.showDefinition(observed.actions.managedDefinitionId, observed.kind);
              }}
              onImportSkill={(observed) => void w.discovery.importSkill(observed)}
              onImportMcp={(observed) => void w.discovery.importMcp(observed)}
              onTakeover={(observed) => void w.discovery.requestTakeover(observed)}
              onRepair={(ids) => void w.plans.repair(ids)}
            />
          </>}
        </div>
      ))}
    </ExtensionDialog>
  );
}
