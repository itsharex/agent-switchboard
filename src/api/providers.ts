import { invoke } from "./client";
import type { AppKind, ModelOptions, UpstreamProtocol } from "./shared";
import type { UsageQuery } from "./usage";
import type { FilePreview } from "./switching";

export interface ProviderProfile {
  id: string;
  app: AppKind;
  routeMode: "official" | "custom";
  name: string;
  model: string | null;
  baseUrl: string | null;
  apiKey: string;
  upstreamProtocol: UpstreamProtocol | null;
  maxOutputTokens: number | null;
  modelOptions: ModelOptions | null;
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
  maxOutputTokens: number | null;
  modelOptions: ModelOptions | null;
  notes?: string | null;
  websiteUrl: string | null;
  usageQuery?: UsageQuery | null;
  officialQuotaRefreshIntervalMinutes?: number | null;
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

export function importDiscoveredProfile(target: AppKind): Promise<ProviderRecord> {
  return invoke<ProviderRecord>("import_discovered_profile", { target });
}

export function probeEndpoint(url: string): Promise<ProbeResult> {
  return invoke<ProbeResult>("probe_endpoint", { url });
}

/** One model from the provider's configured /v1/models endpoint; the
 * optional vendor groups the editor's model picker. */
export interface ProviderModel {
  id: string;
  ownedBy: string | null;
}

/** Models from the provider's configured /v1/models endpoint. */
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
