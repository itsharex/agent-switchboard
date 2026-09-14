import type { AppKind, ExtensionListItem, PlanRequestOperation } from "../../api/client";

export const EXTENSION_CLIENTS: AppKind[] = ["codex", "claude"];

export function itemSupportsClient(item: ExtensionListItem, client: AppKind): boolean {
  if (item.kind === "skill") return item.hostScoped === null || item.hostScoped === client;
  return item.transport === "stdio" || item.transport === "http" || client === "claude";
}

export function clientBindingState(item: ExtensionListItem, client: AppKind) {
  const bindings = item.bindings.filter((binding) => binding.target.client === client);
  const enabled = bindings.filter((binding) => binding.desired === "enabled").length;
  return {
    bindings,
    enabled,
    all: bindings.length > 0 && enabled === bindings.length,
    partial: enabled > 0 && enabled < bindings.length,
  };
}

export function clientDeployState(items: ExtensionListItem[], client: AppKind) {
  const applicable = items.filter((item) => itemSupportsClient(item, client));
  const states = applicable.map((item) => clientBindingState(item, client));
  const enabled = states.filter((state) => state.enabled > 0).length;
  const all = states.length > 0 && states.every((state) => state.all);
  return { applicable: applicable.length, enabled, all, partial: enabled > 0 && !all };
}

// A client control covers its existing scopes. Only an unbound client gets
// a new user-level install; project bindings keep their original targets.
export function clientDeploymentOperations(
  items: ExtensionListItem[],
  client: AppKind,
  enable: boolean,
): PlanRequestOperation[] {
  return items
    .filter((item) => itemSupportsClient(item, client))
    .flatMap<PlanRequestOperation>((item) => {
      const { bindings } = clientBindingState(item, client);
      if (enable && bindings.length === 0) {
        return [{ operation: "install", definitionId: item.id, targets: [{ scope: "app", client }] }];
      }
      return bindings
        .filter((binding) => binding.desired !== (enable ? "enabled" : "disabled"))
        .map((binding) => ({ operation: enable ? "enable" : "disable", bindingId: binding.id }));
    });
}

export function needsDisableScope(operation: PlanRequestOperation, items: ExtensionListItem[]) {
  return (
    operation.operation === "disable" &&
    operation.sharedSettings == null &&
    items.some(
      (item) =>
        item.kind === "skill" &&
        item.bindings.some(
          (binding) =>
            binding.id === operation.bindingId &&
            binding.target.client === "claude" &&
            binding.target.scope === "projectShared",
        ),
    )
  );
}
