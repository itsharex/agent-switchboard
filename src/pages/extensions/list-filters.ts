import type { AppKind, ExtensionListItem } from "../../api/client";
import { itemSupportsClient } from "../../app/extensions/deployment-state";

export function matchesClient(item: ExtensionListItem, client: AppKind): boolean {
  if (item.bindings.some((binding) => binding.target.client === client)) return true;
  return itemSupportsClient(item, client);
}

export function searchNeedle(item: ExtensionListItem): string {
  if (item.kind === "skill") {
    return [item.name, item.manifest.name, item.manifest.description ?? ""].join(" ");
  }
  return [item.name, item.transport, item.transport === "stdio" ? item.command : ""].join(" ");
}
