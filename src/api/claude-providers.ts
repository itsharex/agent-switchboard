import { invoke } from "./client";
import type { ProviderDraft, ProviderRecord } from "./providers";
import type { UpstreamProtocol } from "./shared";

export interface ClaudePresetVariable { label: string; placeholder: string; defaultValue: string | null; editorValue: string }
export interface ClaudePresetSummary {
  id: string;
  name: string;
  websiteUrl: string;
  apiKeyUrl: string | null;
  category: string;
  authentication: "official" | "api_key" | "native_sdk" | "github_copilot" | "codex_oauth" | "xai_oauth";
  upstreamProtocol: UpstreamProtocol | null;
  endpointCandidates: string[];
  variables: Record<string, ClaudePresetVariable>;
  unavailableReason: string | null;
}
export interface ClaudePresetPreparation { draft: ProviderDraft; warnings: string[] }
export const listClaudePresets = (): Promise<ClaudePresetSummary[]> => invoke("list_claude_presets");
export const prepareClaudePreset = (
  presetId: string, apiKey: string, variables: Record<string, string>, accountId: string | null,
): Promise<ClaudePresetPreparation> => invoke("prepare_claude_preset", { presetId, apiKey, variables, accountId });
export const duplicateClaudeProfile = (
  profileId: string, name: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<ProviderRecord> => invoke("duplicate_claude_profile", { profileId, name, expectedFileHash, confirmWrite });
export const searchClaudeProfiles = (query: string): Promise<ProviderRecord[]> => invoke("search_claude_profiles", { query });
