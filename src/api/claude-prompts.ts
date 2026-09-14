import { invoke } from "./client";

export interface ClaudePromptDraft { name: string; description: string | null; content: string }
export interface ClaudePrompt { id: string; draft: ClaudePromptDraft }
export interface ClaudePromptsView {
  fileHash: string;
  prompts: ClaudePrompt[];
  activePromptId: string | null;
  liveHash: string;
  externalChange: boolean;
  pendingContent: boolean;
  recoveryRequired: boolean;
}
export interface ClaudePromptActivation {
  promptId: string | null;
  fileHash: string;
  liveHash: string;
  liveExists: boolean;
  renderedHash: string;
}
export interface ClaudePromptPreview { plan: ClaudePromptActivation; before: string; after: string }

export const listClaudePrompts = (): Promise<ClaudePromptsView> => invoke("list_claude_prompts");
export const saveClaudePrompt = (
  promptId: string | null, draft: ClaudePromptDraft, expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudePromptsView> => invoke("save_claude_prompt", { promptId, draft, expectedFileHash, confirmWrite });
export const removeClaudePrompt = (
  promptId: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudePromptsView> => invoke("remove_claude_prompt", { promptId, expectedFileHash, confirmWrite });
export const reorderClaudePrompts = (
  orderedIds: string[], expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudePromptsView> => invoke("reorder_claude_prompts", { orderedIds, expectedFileHash, confirmWrite });
export const previewClaudePrompt = (
  promptId: string | null, expectedFileHash: string,
): Promise<ClaudePromptPreview> => invoke("preview_claude_prompt", { promptId, expectedFileHash });
export const activateClaudePrompt = (
  plan: ClaudePromptActivation, confirmWrite: boolean,
): Promise<ClaudePromptsView> => invoke("activate_claude_prompt", { plan, confirmWrite });
export const recoverClaudePrompt = (confirmWrite: boolean): Promise<ClaudePromptsView> =>
  invoke("recover_claude_prompt", { confirmWrite });

export interface ClaudePromptSource {
  sourceRevision: string;
  prompts: { sourceId: string; draft: ClaudePromptDraft; enabledInSource: boolean }[];
}
export interface ClaudePromptImportResult { view: ClaudePromptsView; imported: number; unchanged: number; warnings: string[] }
export const scanClaudePromptSource = (sourcePath: string): Promise<ClaudePromptSource> =>
  invoke("scan_claude_prompt_source", { sourcePath });
export const importClaudePromptSource = (
  sourcePath: string, sourceIds: string[], sourceRevision: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudePromptImportResult> => invoke("import_claude_prompt_source", { sourcePath, sourceIds, sourceRevision, expectedFileHash, confirmWrite });
