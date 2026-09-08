import type { AppKind, ExtensionListItem } from "../../api/client";
import type { ExtensionsDeps } from "../../app/extensions/extension-ops";
import {
  clientBindingState,
  clientDeploymentOperations,
  clientDeployState,
} from "../../app/extensions/deployment-state";
import { useDiscoverScan } from "../../app/extensions/useDiscoverScan";
import { useExtensions } from "../../app/useExtensions";
import { matchesClient, searchNeedle } from "./list-filters";
import { useExtensionPlans } from "./useExtensionPlans";
import { useExtensionView, type ExtensionNavigation } from "./useExtensionView";
import { useSkillUpdates } from "./useSkillUpdates";

export function useExtensionWorkspace(deps: ExtensionsDeps & ExtensionNavigation) {
  const ext = useExtensions(deps);
  const discovery = useDiscoverScan({
    refresh: ext.refresh,
    runExclusive: ext.runExclusive,
    onError: deps.onError,
  });
  const nav = useExtensionView(deps);
  const plans = useExtensionPlans(ext, discovery);
  const updates = useSkillUpdates(ext, plans);
  const items = ext.workspace?.items ?? [];
  const kindItems = items.filter((item) => item.kind === nav.kind);
  const visible = kindItems.filter(
    (item) =>
      (nav.client === "all" || matchesClient(item, nav.client)) &&
      searchNeedle(item).toLowerCase().includes(nav.search.trim().toLowerCase()),
  );
  const projectNames = new Map(ext.workspace?.projects.map((project) => [project.id, project.displayName]));
  const resourceNames = new Map(items.map((item) => [item.id, item.name]));
  const bindingInfo = new Map(
    items.flatMap((item) =>
      item.bindings.map(
        (binding) =>
          [binding.id, { name: item.name, kind: item.kind, client: binding.target.client }] as const,
      ),
    ),
  );
  const writeBlocked = deps.busy || !ext.workspace || ext.workspace.recoveryRequired.length > 0;
  const toggleClient = (item: ExtensionListItem, client: AppKind) =>
    plans.prepare({
      operations: clientDeploymentOperations([item], client, !clientBindingState(item, client).all),
    });
  const toggleAll = (client: AppKind) =>
    plans.prepare({
      operations: clientDeploymentOperations(kindItems, client, !clientDeployState(kindItems, client).all),
    });
  const edit = async (item: ExtensionListItem) => {
    if (item.kind === "skill") nav.showDefinition(item.id, "skill", true);
    else {
      const envelope = await ext.loadMcpEdit(item.id);
      if (envelope) nav.setDialog({ type: "mcpEditor", envelope });
    }
  };
  return {
    ext,
    discovery,
    nav,
    plans,
    updates,
    items,
    kindItems,
    visible,
    projectNames,
    resourceNames,
    bindingInfo,
    busy: deps.busy,
    writeBlocked,
    toggleClient,
    toggleAll,
    edit,
  };
}

export type ExtensionWorkspace = ReturnType<typeof useExtensionWorkspace>;
