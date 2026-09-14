import type {
  CommandError,
  ExtensionMutation,
  ExtensionsWorkspace,
} from "../../api/client";
import { toast } from "../../components/use-toast";

export interface ExtensionsDeps {
  busy: boolean;
  setBusy: (busy: boolean) => void;
  clearError: () => void;
  onError: (error: CommandError) => void;
}

/** A committed definition is distinct from its verified deployment state. */
export interface SkillUpdatePreparation {
  definition: ExtensionMutation;
  deployment: "required" | "notRequired" | "unverified";
}

/** Result of advancing a batch of skill definitions to new content
 * versions: the ids that advanced and the per-item failures. */
export interface SkillBatchAdvance {
  advanced: string[];
  failed: Array<{ definitionId: string; message: string }>;
  workspace: ExtensionsWorkspace | null;
}

export type EditPreparation = SkillUpdatePreparation;

/** Serialises one mutation into the shared operation frame; `null` marks a
 * rejected or failed action whose error already surfaced. */
export type ExclusiveRunner = <T>(action: () => Promise<T>) => Promise<T | null>;

/** Reads may run during a write so refreshed editors can load new revisions. */
export type ExtensionReader = <T>(action: () => Promise<T>) => Promise<T | null>;

/** Re-reads the extension workspace after a mutation. */
export type WorkspaceRefresher = () => Promise<ExtensionsWorkspace | null>;

export async function refreshLibraryWrite(
  refresh: WorkspaceRefresher,
  notification: { title: string; description?: string; kind?: "info" | "success" },
): Promise<ExtensionsWorkspace | null> {
  const workspace = await refresh();
  toast(workspace === null
    ? {
        kind: "warning",
        title: `${notification.title}，状态刷新失败`,
        description: "未能读取最新扩展状态，请刷新扩展库后确认结果。",
      }
    : { kind: "success", ...notification });
  return workspace;
}

export function definitionUpdatePreparation(
  definition: ExtensionMutation,
  workspace: ExtensionsWorkspace | null,
): SkillUpdatePreparation {
  const item = workspace?.items.find((entry) => entry.id === definition.id);
  if (!item) {
    toast({
      kind: "warning",
      title: "定义已保存，部署状态未确认",
      description: "未能读取保存后的扩展，请刷新扩展库后重新部署当前版本。",
    });
    return { definition, deployment: "unverified" };
  }
  const required = item.kind === "mcp"
    ? item.bindings.length > 0
    : item.bindings.some((binding) => binding.desired === "enabled" && !binding.lockedDigest);
  if (!required) toast({ kind: "success", title: "定义已保存，没有需要更新的客户端部署" });
  return { definition, deployment: required ? "required" : "notRequired" };
}
