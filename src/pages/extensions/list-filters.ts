import type { AppKind, ExtensionListItem } from "../../api/client";
import { FILE_STATE_LABELS, mcpSupportsClient } from "../../components/extensions/labels";

export type ClientFilter = "all" | AppKind;
export type AddMode = "newMcp" | "newSkill" | "skillSource" | "discover" | null;

export const CLIENT_FILTER_OPTIONS = [
  { value: "all", label: "全部客户端" },
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
] as const;

/** Worst-state-first ordering: the summary names the state that needs
 * attention before the reassuring ones. */
const FILE_STATE_SEVERITY: Array<ExtensionListItem["bindings"][number]["fileState"]> = [
  "unreadable",
  "externalChange",
  "missing",
  "pendingApply",
  "notDeployed",
  "inSync",
];

export function matchesClient(item: ExtensionListItem, client: AppKind): boolean {
  if (item.bindings.some((binding) => binding.target.client === client)) return true;
  if (item.kind === "skill") {
    return (item.hostScoped ?? null) === null || item.hostScoped === client;
  }
  return mcpSupportsClient(item.transport, client);
}

export function clientSummary(item: ExtensionListItem, client: AppKind): string {
  const rows = item.bindings.filter((binding) => binding.target.client === client);
  if (rows.length === 0) return "未部署";
  const worst =
    FILE_STATE_SEVERITY.find((state) => rows.some((binding) => binding.fileState === state)) ??
    "inSync";
  const anyDisabled = rows.some((binding) => binding.desired === "disabled");
  return anyDisabled ? `${FILE_STATE_LABELS[worst]} · 已停用` : FILE_STATE_LABELS[worst];
}

export function searchNeedle(item: ExtensionListItem): string {
  if (item.kind === "skill") {
    return [item.name, item.manifest.name, item.manifest.description ?? ""].join(" ");
  }
  return item.name;
}
