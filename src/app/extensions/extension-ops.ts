import type {
  CommandError,
  ExtensionMutation,
  ExtensionPlanView,
  ExtensionsWorkspace,
} from "../../api/client";

export interface ExtensionsDeps {
  busy: boolean;
  setBusy: (busy: boolean) => void;
  clearError: () => void;
  onError: (error: CommandError) => void;
}

export interface SkillUpdatePreparation {
  definition: ExtensionMutation;
  plan: ExtensionPlanView | null;
}

/** Result of advancing a batch of skill definitions to new content
 * versions: the ids that advanced and the per-item failures. */
export interface SkillBatchAdvance {
  advanced: string[];
  failed: Array<{ definitionId: string; message: string }>;
}

export type EditPreparation = SkillUpdatePreparation;

/** Serialises one mutation into the shared operation frame; `null` marks a
 * rejected or failed action whose error already surfaced. */
export type ExclusiveRunner = <T>(action: () => Promise<T>) => Promise<T | null>;

/** Re-reads the extension workspace after a mutation. */
export type WorkspaceRefresher = () => Promise<ExtensionsWorkspace | null>;
