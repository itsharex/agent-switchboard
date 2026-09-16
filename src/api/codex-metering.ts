import { invoke } from "./client";
import type { UpstreamProtocol } from "./shared";
export interface CodexBilling {
  costMultiplier: string;
  modelSource: "request" | "response";
  dailyLimitUsd: string | null;
  monthlyLimitUsd: string | null;
}
export interface CodexModelPrice {
  inputUsdPerMillion: string;
  outputUsdPerMillion: string;
  cacheReadUsdPerMillion: string;
  cacheCreationUsdPerMillion: string;
  source: string;
}
export interface CodexMeteringSettings {
  version: 1;
  prices: Record<string, CodexModelPrice>;
  providers: Record<string, CodexBilling>;
}
export interface CodexMeteringSnapshot { settings: CodexMeteringSettings; revision: string }
export interface CodexLedgerFilter {
  profileId?: string | null;
  model?: string | null;
  fromMs?: number | null;
  untilMs?: number | null;
  outcome?: "succeeded" | "failed" | null;
}
export interface CodexRequestRecord {
  id: string;
  origin: "proxy" | "session";
  threadId?: string;
  atMs: number;
  billable: boolean;
  profileId: string | null;
  routeRevision: string | null;
  upstreamProtocol: UpstreamProtocol | null;
  requestModel: string | null;
  mappedModel: string | null;
  responseModel: string | null;
  status: number | null;
  durationMs: number;
  firstByteLatencyMs: number | null;
  firstTokenLatencyMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheCreationTokens: number | null;
  reasoningTokens: number | null;
  billing: CodexBilling;
  cost: { model: string; source: string; multiplier: string; totalUsd: string } | null;
  pricingError: string | null;
  attempts: Array<{ profileId: string; routeRevision: string; upstreamProtocol: UpstreamProtocol;
    status: number | null; retryable: boolean }>;
}
export interface CodexLedgerPage { total: number; records: CodexRequestRecord[] }
export interface CodexLedgerSummary {
  requests: number;
  failedRequests: number;
  pricedRequests: number;
  unpricedRequests: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  reasoningTokens: number;
  estimatedUsd: string;
  days: Array<{ startMs: number; requests: number; estimatedUsd: string; unpricedRequests: number }>;
}
export const getCodexMetering = (): Promise<CodexMeteringSnapshot> => invoke("get_codex_metering");
export const setCodexMetering = (settings: CodexMeteringSettings, expectedRevision: string,
  confirmWrite: boolean): Promise<CodexMeteringSnapshot> =>
  invoke("set_codex_metering", { settings, expectedRevision, confirmWrite });
export const getCodexRequestLedger = (filter: CodexLedgerFilter | null, offset: number,
  limit: number): Promise<CodexLedgerPage> => invoke("get_codex_request_ledger", { filter, offset, limit });
export const getCodexRequestSummary = (filter: CodexLedgerFilter | null): Promise<CodexLedgerSummary> =>
  invoke("get_codex_request_summary", { filter });
export const repriceCodexRequests = (filter: CodexLedgerFilter | null, expectedRevision: string,
  confirmWrite: boolean): Promise<number> => invoke("reprice_codex_requests", { filter, expectedRevision, confirmWrite });
export interface CodexSessionSyncReport {
  filesScanned: number;
  imported: number;
  skipped: number;
  suspectedDuplicates: number;
  deferredFiles: number;
  errors: string[];
}
export interface CodexSessionRebuildOutcome {
  backupPath: string | null;
  report: CodexSessionSyncReport;
}
export const syncCodexSessionUsage = (): Promise<CodexSessionSyncReport> =>
  invoke("sync_codex_session_usage");
export const rebuildCodexSessionUsage = (confirmWrite: boolean): Promise<CodexSessionRebuildOutcome> =>
  invoke("rebuild_codex_session_usage", { confirmWrite });
