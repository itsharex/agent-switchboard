import { invoke } from "./client";
import type { CodexProviderDraft, CodexProviderRecord, CodexUpstream } from "./providers";

export interface CodexPresetSummary {
  id: string;
  name: string;
  category: string;
  websiteUrl: string;
  apiKeyUrl: string | null;
  authentication: "apiKey" | "nativeOpenAi" | "managedXai";
  upstream: CodexUpstream | null;
  endpointCandidates: string[];
  models: string[];
}
export interface CodexPresetPreparation { draft: CodexProviderDraft; warnings: string[] }

export const listCodexPresets = (): Promise<CodexPresetSummary[]> => invoke("list_codex_presets");
export const prepareCodexPreset = (presetId: string, apiKey: string): Promise<CodexPresetPreparation> =>
  invoke("prepare_codex_preset", { presetId, apiKey });
export const searchCodexProfiles = (query: string): Promise<CodexProviderRecord[]> =>
  invoke("search_codex_profiles", { query });
export const duplicateCodexProfile = (profileId: string, expectedFileHash: string, name: string | null = null): Promise<CodexProviderRecord> =>
  invoke("duplicate_codex_profile", { profileId, expectedFileHash, name });

/** Drops a prepared Codex edit without saving the provider or native files. */
export const cancelCodexProfileSave = (preparationId: string): Promise<void> => invoke("cancel_codex_profile_save", { preparationId });
