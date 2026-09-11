import { invoke } from "./client";
import type { AppKind, ModelOptions, ResponsesOptions, UpstreamProtocol } from "./shared";
import type { UsageQuery } from "./usage";
import type { FilePreview } from "./switching";
import type { SettingsValues } from "./settings";

export interface ProviderProfile {
  id: string;
  app: AppKind;
  routeMode: "official" | "custom";
  name: string;
  model: string | null;
  baseUrl: string | null;
  apiKey: string;
  upstreamProtocol: UpstreamProtocol | null;
  responsesOptions: ResponsesOptions | null;
  maxOutputTokens: number | null;
  modelOptions: ModelOptions | null;
  parameters: SettingsValues;
  /** Local-only note; never written into any client configuration. */
  notes?: string | null;
  /** Provider homepage, used for navigation only. */
  websiteUrl: string | null;
  /** Application-side usage-balance query; never written into client config. */
  usageQuery?: UsageQuery | null;
  /** Whole minutes between official Codex quota panel re-queries; absent or
   * null keeps the panel manual-only. Never written into client config. */
  officialQuotaRefreshIntervalMinutes?: number | null;
}

/** One provider file together with its storage revision. `fileHash` guards
 * every mutation of that provider file against an external change. */
export interface ProviderRecord {
  profile: ProviderProfile;
  fileHash: string;
}

/** Routing is explicit. Official profiles retain no endpoint, API key, model
 * override, or usage script because client credentials stay native. */
export interface ProviderDraft {
  app: AppKind;
  routeMode: "official" | "custom";
  name: string;
  model: string | null;
  baseUrl: string | null;
  apiKey: string;
  upstreamProtocol: UpstreamProtocol | null;
  responsesOptions: ResponsesOptions | null;
  maxOutputTokens: number | null;
  modelOptions: ModelOptions | null;
  parameters: SettingsValues;
  notes?: string | null;
  websiteUrl: string | null;
  usageQuery?: UsageQuery | null;
  officialQuotaRefreshIntervalMinutes?: number | null;
}

export type CodexUpstream = "responses" | "chatCompletions" | "anthropicMessages";
export type CodexRequestMode = ResponsesOptions["requestMode"];

export type CodexChatReasoning =
  | { kind: "unsupported" }
  | {
    kind: "configured";
    thinkingParameter: "none" | "thinking" | "enableThinking" | "reasoningSplit";
    effortParameter: "none" | "reasoningEffort" | "reasoningObject";
    effortMode: "passthrough" | "lowHigh" | "deepSeek" | "openRouter";
  };

export interface CodexCapabilities {
  responses: boolean;
  compact: boolean;
  models: boolean;
  chatCompletions: boolean;
  alphaSearch: boolean;
  imageGeneration: boolean;
  imageEdit: boolean;
  functionTools: boolean;
  customTools: boolean;
  toolSearch: boolean;
  reasoning: boolean;
  chatReasoning: CodexChatReasoning;
}

export interface CodexCatalogEntry {
  id: string;
  contextWindow: number;
  maxOutputTokens: number;
  functionTools: boolean;
  customTools: boolean;
  toolSearch: boolean;
  reasoning: boolean;
  defaultReasoningLevel: CodexReasoningLevel;
  supportedReasoningLevels: CodexReasoningLevel[];
  images: boolean;
  compact: boolean;
}

export type CodexReasoningLevel = "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra";

export interface CodexModelRoute {
  clientModel: string;
  upstreamModel: string;
}

/** The current, complete third-party Codex contract. Official login is not
 * represented by a profile. */
export interface CodexProviderProfile {
  id: string;
  name: string;
  endpoint: string;
  apiKey: string;
  upstream: CodexUpstream;
  requestMode: CodexRequestMode;
  defaultModel: string;
  catalog: CodexCatalogEntry[];
  modelRoutes: CodexModelRoute[];
  capabilities: CodexCapabilities;
}

export interface CodexProviderDraft {
  name: string;
  endpoint: string;
  apiKey: string;
  upstream: CodexUpstream;
  requestMode: CodexRequestMode;
  defaultModel: string;
  catalog: CodexCatalogEntry[];
  modelRoutes: CodexModelRoute[];
  capabilities: CodexCapabilities;
  parameters: SettingsValues;
  notes: string | null;
  websiteUrl: string | null;
  usageQuery: UsageQuery | null;
}

export interface CodexProviderRecord {
  profile: CodexProviderProfile;
  parameters: SettingsValues;
  notes: string | null;
  websiteUrl: string | null;
  usageQuery: UsageQuery | null;
  fileHash: string;
}

/** Reachability grade of one manual probe: any HTTP answer counts as ok/slow,
 * only network-level failures (DNS / refused / TLS / timeout) are unreachable. */
export type ProbeGrade = "ok" | "slow" | "unreachable";

export interface ProbeResult {
  grade: ProbeGrade;
  status: number | null;
  latencyMs: number | null;
  error: string | null;
  at: string;
}

export type ProfileSaveKind = "create" | "noChange" | "saveOnly" | "saveAndApply";

/** Backend-owned classification of an edit against the stored profile and
 * the current client configuration. A candidate exists only for an edit that
 * must re-apply the active provider. */
export interface ProfileSavePreparation {
  preparationId: string;
  kind: ProfileSaveKind;
  preview: FilePreview | null;
}

export function listProfiles(): Promise<ProviderRecord[]> {
  return invoke<ProviderRecord[]>("list_profiles");
}

export function listCodexProfiles(): Promise<CodexProviderRecord[]> {
  return invoke<CodexProviderRecord[]>("list_codex_profiles");
}

export function createCodexProfile(draft: CodexProviderDraft): Promise<CodexProviderRecord> {
  return invoke<CodexProviderRecord>("create_codex_profile", { draft });
}

/** Prepares an existing Codex profile save. A live route change returns the
 * exact client-file preview that must be confirmed before it can be applied. */
export function prepareCodexProfileSave(
  profileId: string,
  draft: CodexProviderDraft,
  expectedFileHash: string,
): Promise<ProfileSavePreparation> {
  return invoke<ProfileSavePreparation>("prepare_codex_profile_save", {
    profileId,
    draft,
    expectedFileHash,
  });
}

export function commitCodexProfileSave(
  preparationId: string,
  confirmWrite: boolean,
): Promise<CodexProviderRecord> {
  return invoke<CodexProviderRecord>("commit_codex_profile_save", {
    preparationId,
    confirmWrite,
  });
}

export function deleteCodexProfile(profileId: string, expectedFileHash: string): Promise<void> {
  return invoke<void>("delete_codex_profile", { profileId, expectedFileHash });
}

export function reorderCodexProfiles(
  orderedIds: string[],
  expectedFileHashes: Record<string, string>,
): Promise<CodexProviderRecord[]> {
  return invoke<CodexProviderRecord[]>("reorder_codex_profiles", {
    orderedIds,
    expectedFileHashes,
  });
}

export function resetProfileStore(confirmWrite: boolean): Promise<void> {
  return invoke<void>("reset_profile_store", { confirmWrite });
}

export function prepareProfileSave(
  profileId: string | null,
  draft: ProviderDraft,
  expectedFileHash: string | null,
): Promise<ProfileSavePreparation> {
  return invoke<ProfileSavePreparation>("prepare_profile_save", { profileId, draft, expectedFileHash });
}

export function commitProfileSave(
  preparationId: string,
  confirmWrite: boolean,
): Promise<ProviderRecord> {
  return invoke<ProviderRecord>("commit_profile_save", { preparationId, confirmWrite });
}

export function deleteProfile(profileId: string, expectedFileHash: string): Promise<void> {
  return invoke<void>("delete_profile", { profileId, expectedFileHash });
}

export function reorderProfiles(
  target: AppKind,
  orderedIds: string[],
  expectedFileHashes: Record<string, string>,
): Promise<ProviderRecord[]> {
  return invoke<ProviderRecord[]>("reorder_profiles", {
    target,
    orderedIds,
    expectedFileHashes,
  });
}

export function importDiscoveredClaudeProfile(): Promise<ProviderRecord> {
  return invoke<ProviderRecord>("import_discovered_claude_profile");
}

export function probeEndpoint(url: string): Promise<ProbeResult> {
  return invoke<ProbeResult>("probe_endpoint", { url });
}

export interface ProviderEndpoints {
  requestUrl: string;
  modelsUrl: string;
}

/** Pure backend resolution, without contacting the provider or reading credentials. */
export function resolveProviderEndpoints(
  baseUrl: string,
  upstreamProtocol: UpstreamProtocol,
): Promise<ProviderEndpoints> {
  return invoke("resolve_provider_endpoints", { request: { baseUrl, upstreamProtocol } });
}

/** One model from the provider's configured models endpoint; the
 * optional vendor groups the editor's model picker. */
export interface ProviderModel {
  id: string;
  ownedBy: string | null;
}

/** Models from the provider's configured API root. */
export function fetchProviderModels(
  baseUrl: string,
  apiKey: string,
  upstreamProtocol: UpstreamProtocol,
): Promise<ProviderModel[]> {
  return invoke<ProviderModel[]>("fetch_provider_models", {
    request: {
      url: baseUrl,
      apiKey,
      upstreamProtocol,
    },
  });
}
