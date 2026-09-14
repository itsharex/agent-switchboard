import { invoke } from "./client";
import type { UpstreamProtocol } from "./shared";

export type ClaudeFailoverOutcome = "success" | "retryablefailure" | "failure";

export interface ClaudeFailoverAttempt {
  profileId: string;
  routeRevision: string;
  upstreamProtocol: UpstreamProtocol;
  status: number | null;
  outcome: ClaudeFailoverOutcome;
}

export interface ClaudeRequestRecord {
  at: string;
  profileId: string | null;
  routeRevision: string | null;
  clientProtocol: UpstreamProtocol;
  upstreamProtocol: UpstreamProtocol | null;
  requestModel: string | null;
  mappedModel: string | null;
  responseModel: string | null;
  cost: ClaudeRequestCost | null;
  pricingError: string | null;
  firstTokenLatencyMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheCreationTokens: number | null;
  reasoningTokens: number | null;
  status: number | null;
  durationMs: number;
  firstByteLatencyMs: number | null;
  failoverAttempts: ClaudeFailoverAttempt[];
}

export interface ClaudeRequestLedgerPage {
  entries: ClaudeRequestRecord[];
  offset: number;
  limit: number;
  total: number;
  hasMore: boolean;
}

export interface ClaudeRequestLedgerSummary {
  totalRequests: number;
  failedRequests: number;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheCreationTokens: number | null;
  reasoningTokens: number | null;
  totalDurationMs: number;
  averageDurationMs: number | null;
  averageFirstByteLatencyMs: number | null;
  averageFirstTokenLatencyMs: number | null;
  estimatedCostUsd: string;
  pricedRequests: number;
  unpricedRequests: number;
}

export function getClaudeRequestLedger(
  offset = 0,
  limit = 50,
  filter: ClaudeLedgerFilter | null = null,
): Promise<ClaudeRequestLedgerPage> {
  return invoke<ClaudeRequestLedgerPage>("get_claude_request_ledger", { offset, limit, filter });
}

export function getClaudeRequestLedgerSummary(filter: ClaudeLedgerFilter | null = null): Promise<ClaudeRequestLedgerSummary> {
  return invoke<ClaudeRequestLedgerSummary>("get_claude_request_ledger_summary", { filter });
}


export interface ClaudeRequestCost { model: string; source: string; multiplier: string; totalUsd: string }
export interface ClaudeLedgerFilter {
  profileId?: string | null;
  model?: string | null;
  from?: string | null;
  to?: string | null;
  failuresOnly?: boolean;
}
export interface ClaudeModelPrice {
  inputUsdPerMillion: string;
  outputUsdPerMillion: string;
  cacheReadUsdPerMillion: string;
  cacheCreationUsdPerMillion: string;
  source: string;
}
export interface ClaudePriceBook { version: 1; models: Record<string, ClaudeModelPrice> }
export interface ClaudePriceBookSnapshot { book: ClaudePriceBook; fileHash: string }
export const getClaudePriceBook = (): Promise<ClaudePriceBookSnapshot> => invoke("get_claude_price_book");
export const setClaudePriceBook = (book: ClaudePriceBook, expectedFileHash: string, confirmWrite: boolean): Promise<ClaudePriceBookSnapshot> =>
  invoke("set_claude_price_book", { book, expectedFileHash, confirmWrite });
