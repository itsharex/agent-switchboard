import { invoke } from "./client";
import type { GlobalPromptDocument } from "./settings";
export interface CodexPromptDraft { name: string; description: string | null; content: string }
export interface CodexPromptPreset { id: string; draft: CodexPromptDraft; createdAt: string; updatedAt: string }
export interface CodexPromptsView { revision: string; activeId: string | null; presets: CodexPromptPreset[]; live: GlobalPromptDocument; pendingRecovery: boolean }
export interface CodexPromptActivation { presetId: string | null; revision: string; liveHash: string; renderedHash: string }
export interface CodexPromptPreview { plan: CodexPromptActivation; before: string; after: string; backfillsActive: boolean }
export const listCodexPrompts = (): Promise<CodexPromptsView> => invoke("list_codex_prompts");
export const saveCodexPrompt = (presetId: string | null, draft: CodexPromptDraft, expectedRevision: string, expectedLiveHash: string | null = null, confirmWrite = false): Promise<CodexPromptsView> =>
  invoke("save_codex_prompt", { presetId, draft, expectedRevision, expectedLiveHash, confirmWrite });
export const deleteCodexPrompt = (presetId: string, expectedRevision: string, confirmWrite: boolean): Promise<CodexPromptsView> =>
  invoke("delete_codex_prompt", { presetId, expectedRevision, confirmWrite });
export const previewCodexPrompt = (presetId: string | null, expectedRevision: string): Promise<CodexPromptPreview> =>
  invoke("preview_codex_prompt", { presetId, expectedRevision });
export const applyCodexPrompt = (plan: CodexPromptActivation, confirmWrite: boolean): Promise<CodexPromptsView> => invoke("apply_codex_prompt", { plan, confirmWrite });
export const recoverCodexPrompt = (confirmWrite: boolean): Promise<CodexPromptsView> => invoke("recover_codex_prompt", { confirmWrite });
