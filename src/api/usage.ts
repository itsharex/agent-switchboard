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
  resetsAt?: string;
  isValid?: boolean;
  invalidMessage?: string;
  extra?: string;
}

/** Complete result of one usage-query response. */
export interface UsageSummary {
  readings: UsageReading[];
  at: string;
}

export interface UsageSnapshot {
  summary: UsageSummary | null;
  error: string | null;
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

/** Runs one persisted provider query right now without exposing its
 * credential to the UI. Successful summaries are retained by the desktop
 * runtime for tray display. This is the forced manual path (the refresh
 * button); the backend scheduler owns automatic re-queries. */
export function queryProfileUsage(target: AppKind, profileId: string): Promise<void> {
  return invoke<void>("query_profile_usage", { target, profileId });
}

/** Reads the last completed query and any retained readings.
 * Null means no attempt has completed; this never contacts upstream. */
export function readProfileUsage(target: AppKind, profileId: string): Promise<UsageSnapshot | null> {
  return invoke<UsageSnapshot | null>("read_profile_usage", { target, profileId });
}

/** Refreshes the official profile's account-bound cache without accepting
 * renderer credentials, endpoints, or raw OAuth account data. */
export function queryCodexOfficialQuota(profileId: string): Promise<void> {
  return invoke<void>("query_codex_official_quota", { profileId });
}

/** Shares the account-checked cache projection used by the tray. */
export function readCodexOfficialQuota(profileId: string): Promise<CodexOfficialQuota | null> {
  return invoke<CodexOfficialQuota | null>("read_codex_official_quota", { profileId });
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

/** The public feed's coarse per-UTC-day reset history (levels 0–4); days
 * without an entry had no signal. */
export interface CodexResetHeatmap {
  timezone: string;
  weeks: number;
  total: number;
  days: CodexResetHeatmapDay[];
}

export interface CodexResetHeatmapDay {
  date: string;
  count: number;
  level: number;
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
  heatmap: CodexResetHeatmap;
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

// ------------------------------------------------------------- codex probe

/** `questionId` that selects the user-authored question. */
export const CUSTOM_PROBE_QUESTION_ID = "custom";

/** One selectable probe question from the backend-owned catalog. */
export interface CodexProbeQuestion {
  id: string;
  label: string;
}

export type CodexProbeBatchStatus =
  | "running" | "completed" | "cancelled" | "failed" | "interrupted" | "config-changed";

/** Grading outcome of one run; execution failures never grade. */
export type CodexProbeRunStatus = "running" | "passed" | "failed" | "undetermined";

/** One run row as persisted: the session id is saved the moment the CLI
 * reports it; unknown usage stays `null`, a recorded real zero stays zero. */
export interface CodexProbeRun {
  seq: number;
  status: CodexProbeRunStatus;
  sessionId: string | null;
  finalAnswer: string | null;
  reportedModel: string | null;
  durationMs: number | null;
  reasoningTokens: number | null;
  totalTokens: number | null;
  executionError: string | null;
  usageError: string | null;
}

/** The configuration snapshot taken when the batch started. Profile fields
 * are snapshots — renames and deletions never rewrite history, and an
 * unidentified configuration shows as 未关联档案. */
export interface CodexProbeConfigSnapshot {
  profileId: string | null;
  profileName: string | null;
  profileModel: string | null;
  reasoningEffort: string | null;
  connectionIdentity: string | null;
  fingerprint: string;
}

export interface CodexProbeQuestionSnapshot {
  id: string;
  label: string;
  text: string;
  expectedAnswer: string;
}

/** One batch (live or historical) with its runs, read from the local probe
 * history — the only durable source of probe state. */
export interface CodexProbeBatch {
  batchId: string;
  status: CodexProbeBatchStatus;
  startedAt: string;
  finishedAt: string | null;
  plannedRuns: number;
  completedRuns: number;
  question: CodexProbeQuestionSnapshot;
  gradingVersion: string;
  cliVersion: string | null;
  config: CodexProbeConfigSnapshot;
  statusError: string | null;
  runs: CodexProbeRun[];
  /** True when a persistence failure left results only in memory. */
  persistPending: boolean;
  persistError: string | null;
}

export interface CodexProbeRequest {
  runCount: number;
  /** A built-in catalog id, or `custom` with the question and answer. */
  questionId: string;
  customQuestion?: string;
  customAnswer?: string;
}

/** The built-in question catalog; the backend owns the bank. */
export function listCodexProbeQuestions(): Promise<CodexProbeQuestion[]> {
  return invoke<CodexProbeQuestion[]>("list_codex_probe_questions");
}

/** Starts one probe batch against the active Codex configuration. Every run
 * is a real Codex call and spends quota; the batch is recorded in the local
 * history before the first call. */
export function startCodexProbe(request: CodexProbeRequest): Promise<void> {
  return invoke<void>("start_codex_probe", { request });
}

/** The live-or-latest batch. The panel reconnects here after a refresh or
 * restart — the frontend remembers no batch id. */
export function getCurrentCodexProbe(): Promise<CodexProbeBatch | null> {
  return invoke<CodexProbeBatch | null>("get_current_codex_probe");
}

/** Requests cancellation of the running batch; the in-flight run is
 * terminated. */
export function cancelCodexProbe(): Promise<boolean> {
  return invoke<boolean>("cancel_codex_probe");
}

/** Flushes results whose persistence failed mid-batch. */
export function retryCodexProbeSave(): Promise<void> {
  return invoke<void>("retry_codex_probe_save");
}

export type CodexProbeHistoryRange = "all" | "last7Days" | "last30Days";

export type CodexProbeProfileFilter =
  | { kind: "all" } | { kind: "unlinked" } | { kind: "profile"; id: string };

export interface CodexProbeHistoryQuery {
  offset: number;
  limit: number;
  range: CodexProbeHistoryRange;
  profile: CodexProbeProfileFilter;
  /** `all` or omitted means every status. */
  status: CodexProbeBatchStatus | "all";
}

/** One row of the history list. */
export interface CodexProbeHistoryItem {
  batchId: string;
  startedAt: string;
  finishedAt: string | null;
  status: CodexProbeBatchStatus;
  questionLabel: string;
  profileId: string | null;
  profileName: string | null;
  runCount: number;
  passedCount: number;
  judgedCount: number;
  recordedRuns: number;
  totalTokens: number | null;
}

export interface CodexProbeHistoryPage {
  items: CodexProbeHistoryItem[];
  total: number;
  offset: number;
  limit: number;
}

/** History reads are paged and ordered newest-first. */
export function listCodexProbeHistory(query: CodexProbeHistoryQuery): Promise<CodexProbeHistoryPage> {
  return invoke<CodexProbeHistoryPage>("list_codex_probe_history", {
    request: {
      offset: query.offset,
      limit: query.limit,
      days: query.range === "all" ? null : query.range === "last7Days" ? 7 : 30,
      profile: query.profile,
      status: query.status === "all" ? null : query.status,
    },
  });
}

/** Distinct profile snapshots seen in the history, for the filter. */
export function listCodexProbeHistoryProfiles(): Promise<Array<{ profileId: string; profileName: string }>> {
  return invoke<Array<{ profileId: string; profileName: string }>>("list_codex_probe_history_profiles");
}

/** One historical batch with its runs. */
export function getCodexProbeBatch(batchId: string): Promise<CodexProbeBatch | null> {
  return invoke<CodexProbeBatch | null>("get_codex_probe_batch", { batchId });
}

/** Deletes finished batches and their runs — only the radar's own records;
 * Codex sessions and the real usage ledger stay untouched. */
export function deleteCodexProbeBatches(batchIds: string[]): Promise<{ deleted: number }> {
  return invoke<{ deleted: number }>("delete_codex_probe_batches", {
    request: { batchIds },
  });
}
