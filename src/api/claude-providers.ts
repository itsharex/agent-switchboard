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

export interface ClaudeSnippetVisualKey { key: string; value: string }
export interface ClaudeSnippetPreview {
  visual: ClaudeSnippetVisualKey[];
  extraKeys: string[];
  rejected: string[];
}
/** Read-only scan plus the client-settings revision the confirmation echoes back. */
export interface ClaudeSnippetScan {
  found: boolean;
  sourceRevision: string;
  preview: ClaudeSnippetPreview | null;
  settingsRevision: string;
}
export interface ClaudeSnippetImportResult {
  snapshot: { settings: unknown; settingsHash: string };
  visualChanged: number;
  visualUnchanged: number;
  extraChanged: number;
  rejected: string[];
  warnings: string[];
}
export const scanClaudeSnippetSource = (sourcePath: string): Promise<ClaudeSnippetScan> =>
  invoke("scan_claude_snippet_source", { sourcePath });
export const importClaudeSnippetSource = (
  sourcePath: string, sourceRevision: string, expectedSettingsHash: string, confirmWrite: boolean,
): Promise<ClaudeSnippetImportResult> =>
  invoke("import_claude_snippet_source", { sourcePath, sourceRevision, expectedSettingsHash, confirmWrite });

/** One credential-free reachability probe per candidate endpoint, in input order. */
export interface ClaudeEndpointLatency {
  url: string;
  result: import("./providers").ProbeResult | null;
  error: string | null;
}
export const testClaudeEndpoints = (urls: string[]): Promise<ClaudeEndpointLatency[]> =>
  invoke("test_claude_endpoints", { urls });
