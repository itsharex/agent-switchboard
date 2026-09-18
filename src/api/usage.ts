import { invoke } from "./client";
import type { AppKind, UpstreamProtocol } from "./shared";
import type { ProviderConnectionOptions } from "./providers";

/** One declarative usage-balance query: a GET against the provider endpoint
 * with `{{baseUrl}}` / `{{apiKey}}` placeholders plus JSON Pointer paths. */
export interface DeclarativeUsageQuery {
  kind: "declarative";
  url: string;
  remainingPath?: string | null;
  usedPath?: string | null;
  totalPath?: string | null;
  /** Display unit for the extracted numbers, e.g. "USD". */
  unit?: string | null;
  /** Minutes between the desktop scheduler's automatic re-queries of this
   * profile; 0 keeps it manual-only. */
  refreshIntervalMinutes: number;
}

/** A self-authored JavaScript query. The source evaluates to `{ request,
 * extract }`; it is executed in the backend's restricted runtime. */
export interface ScriptUsageQuery {
  kind: "script";
  source: string;
  /** Minutes between the desktop scheduler's automatic re-queries of this
   * profile; 0 keeps it manual-only. */
  refreshIntervalMinutes: number;
}

/** The only persisted usage-query contract. `null` on a profile means the
 * optional feature is not configured. */
export type UsageQuery = DeclarativeUsageQuery | ScriptUsageQuery;

/** One named or unnamed usage reading. */
export interface UsageReading {
  planName?: string;
  remaining: number | null;
  used: number | null;
  total: number | null;
  unit: string | null;
}

/** Complete result of one usage-query response. */
export interface UsageSummary {
  readings: UsageReading[];
  at: string;
}

/** Local-calendar range for the read-only client session usage report. */
export type ModelUsageRange = "today" | "last7Days" | "last30Days" | "all";

/** One local-session token total, grouped by client and model. It is not a
 * provider billing quota and does not assert a remaining allowance. */
export interface ModelUsageGroup {
  app: AppKind;
  model: string | null;
  inputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  outputTokens: number;
  totalTokens: number;
  sessionCount: number;
}

/** Token totals assigned to one local-calendar day. These values remain
 * separate from provider billing or quota data. */
export interface ModelUsageDay {
  date: string;
  inputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  outputTokens: number;
  totalTokens: number;
}

/** Token totals that a local session log did not timestamp. They remain in
 * the exact totals but never become an invented trend point. */
export interface ModelUsageTokens {
  inputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  outputTokens: number;
  totalTokens: number;
}

/** A non-fatal client-local reason why a model usage report is incomplete. */
export interface ModelUsageIssue {
  app: AppKind;
  message: string;
}

/** Credential-free aggregation over the supported local session records. */
export interface ModelUsageReport {
  range: ModelUsageRange;
  generatedAt: string;
  groups: ModelUsageGroup[];
  days: ModelUsageDay[];
  unassignedTokens: ModelUsageTokens;
  issues: ModelUsageIssue[];
}

/** Explicit local-session report cache policy. `forceRefresh` re-scans the
 * approved local session roots instead of returning their saved snapshot. */
export interface ModelUsageRequest {
  range: ModelUsageRange;
  forceRefresh: boolean;
}

/** Whether the report came from its persisted local-session snapshot or this
 * request's completed scan. It does not describe provider quota freshness. */
export type ModelUsageFreshness = "cached" | "fresh";

/** Backend-owned local-session snapshot timing. The UI schedules its next
 * foreground refresh from `refreshAfter`, not from a separate client TTL. */
export interface ModelUsageRead {
  report: ModelUsageReport;
  freshness: ModelUsageFreshness;
  refreshAfter: string;
  cacheWarning: string | null;
}

/** The renderer asks for either its current provider's normalized history or
 * the independent account-level Codex official history. */
export type UsageHistoryRequest =
  | { kind: "provider"; profileId: string }
  | { kind: "official" };

export type UsageHistoryMetric = "remaining" | "used" | "usedPercent";

export interface UsageHistoryPoint {
  at: string;
  value: number;
}

/** A credential-free, unit-aware historical series returned by the desktop
 * service after successful real reads only. */
export interface UsageHistorySeries {
  id: string;
  label: string;
  unit: string | null;
  metric: UsageHistoryMetric;
  points: UsageHistoryPoint[];
}

/** Renderer-safe state of the read-only Codex ChatGPT-login quota service. */
export type CodexOfficialQuotaStatus =
  | "available"
  | "signInRequired"
  | "reauthenticationRequired"
  | "unavailable";

export interface CodexOfficialQuotaWindow {
  label: string;
  usedPercent: number;
  resetsAt: string | null;
}

/** How a locally detected reset relates to the previously declared schedule. */
export type CodexOfficialQuotaResetKind = "scheduled" | "early";

/** One reset observed by comparing consecutive successful official reads. */
export interface CodexOfficialQuotaReset {
  observedAt: string;
  kind: CodexOfficialQuotaResetKind;
  resetsAt: string | null;
}

/** OAuth credentials and account identifiers never appear in this type. */
export interface CodexOfficialQuota {
  status: CodexOfficialQuotaStatus;
  windows: CodexOfficialQuotaWindow[];
  at: string | null;
  stale: boolean;
  lastReset: CodexOfficialQuotaReset | null;
}

/** Runs one on-demand usage-balance query with the supplied profile or editor
 * credential; nothing is persisted and the credential never appears in errors. */
export function testUsageQuery(
  query: UsageQuery,
  apiKey: string,
  baseUrl: string | null,
  upstreamProtocol: UpstreamProtocol,
  authentication?: import("./shared").AuthenticationScheme | null,
  connection?: ProviderConnectionOptions | null,
): Promise<UsageSummary> {
  return invoke<UsageSummary>("test_usage_query", {
    request: {
      query,
      apiKey,
      baseUrl,
      upstreamProtocol,
      ...(authentication ? { authentication } : {}),
      ...(connection ? { connection } : {}),
    },
  });
}

/** Ensures the provider's summary is fresh without exposing its credential
 * to the UI: the desktop runtime returns its retained entry while it is not
 * due, or pulls one query forward into now. Mount-time reads go through
 * here; the backend owns all query timing. */
export function ensureProfileUsage(profileId: string): Promise<UsageSummary> {
  return invoke<UsageSummary>("ensure_profile_usage", { profileId });
}

/** Runs one persisted provider query right now without exposing its
 * credential to the UI. Successful summaries are retained by the desktop
 * runtime for tray display. This is the forced manual path (the refresh
 * button); the backend scheduler owns automatic re-queries. */
export function queryProfileUsage(profileId: string): Promise<UsageSummary> {
  return invoke<UsageSummary>("query_profile_usage", { profileId });
}

/** Reads the retained last successful summary without contacting the
 * provider. `null` until a query succeeds for the profile's current usage
 * query. */
export function readProfileUsage(profileId: string): Promise<UsageSummary | null> {
  return invoke<UsageSummary | null>("read_profile_usage", { profileId });
}

/** Reads the current native Codex official-login quota without accepting any
 * renderer credential, endpoint, or raw OAuth account data. */
export function queryCodexOfficialQuota(profileId: string): Promise<CodexOfficialQuota> {
  return invoke<CodexOfficialQuota>("query_codex_official_quota", { profileId });
}

/** Reads the persisted last successful official read without contacting the
 * network. Absent until the first refresh of the machine's Codex login. */
export function getCachedCodexOfficialReset(): Promise<CodexOfficialQuota | null> {
  return invoke<CodexOfficialQuota | null>("get_cached_codex_official_reset");
}

/** One explicit read of the machine's Codex official login. Account-scoped
 * and profile-independent; failed reads return as statuses, not errors. */
export function refreshCodexOfficialReset(): Promise<CodexOfficialQuota> {
  return invoke<CodexOfficialQuota>("refresh_codex_official_reset");
}

/** Public, global reset signals from Codex Runway. These do not describe the
 * signed-in account's actual quota or entitlement. */
export type CodexResetFeedStatus = "ok" | "degraded";
export type ResetType = "global" | "banked" | "other";

export interface ResetSignal {
  announcedAt: string;
  effectiveAt: string | null;
  schedulePrecision: string | null;
  confidence: number;
  resetType: ResetType;
}

export interface TiboPost {
  announcedAt: string;
  text: string;
  url: string;
}

export interface CodexResetStatus {
  sourceUrl: string;
  feedStatus: CodexResetFeedStatus;
  generatedAt: string;
  lastSuccessfulCheckAt: string;
  checkedAt: string;
  latestConfirmedSignal: ResetSignal | null;
  nextScheduledReset: ResetSignal | null;
  latestRelevantTiboPost: TiboPost | null;
  sourceWarning: string | null;
}

export type CodexResetFreshness = "cached" | "live";

/** The normalized public signal plus how the overview obtained it. */
export interface CodexResetRead {
  status: CodexResetStatus;
  freshness: CodexResetFreshness;
  cacheWarning: string | null;
}

/** Reads only the last successful local snapshot; it never contacts the feed. */
export function getCachedCodexResetStatus(): Promise<CodexResetRead | null> {
  return invoke<CodexResetRead | null>("get_cached_codex_reset_status");
}

export function checkCodexResetStatus(): Promise<CodexResetRead> {
  return invoke<CodexResetRead>("check_codex_reset_status");
}

/** Reads the backend-owned local-session snapshot or explicitly re-scans the
 * two approved roots. */
export function getModelUsageReport(request: ModelUsageRequest): Promise<ModelUsageRead> {
  return invoke<ModelUsageRead>("get_model_usage_report", { request });
}

/** Reads the app-owned, credential-free history ledger. It never triggers a
 * provider or Codex network request. */
export function getUsageHistory(request: UsageHistoryRequest): Promise<UsageHistorySeries[]> {
  return invoke<UsageHistorySeries[]>("get_usage_history", { request });
}
