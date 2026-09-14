import type { AppKind, ExtensionListItem, PlanRequestOperation } from "../../api/client";

export function pendingDeployment(
  operations: PlanRequestOperation[], item: ExtensionListItem, client: AppKind,
): boolean {
  return operations.some((operation) => {
    if (operation.bindingId) return item.bindings.some((binding) =>
      binding.id === operation.bindingId && binding.target.client === client);
    return operation.definitionId === item.id &&
      (operation.targets?.some((target) => target.client === client) ?? true);
  });
}
