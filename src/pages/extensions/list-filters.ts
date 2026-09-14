import type { ExtensionListItem } from "../../api/client";

export function searchNeedle(item: ExtensionListItem): string {
  if (item.kind === "skill") {
    return [item.name, item.manifest.name, item.manifest.description ?? ""].join(" ");
  }
  return [item.name, item.transport, item.transport === "stdio" ? item.command : ""].join(" ");
}
