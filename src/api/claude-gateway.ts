import { invoke } from "./client";
import type { BackupRecord, KeyChange } from "./switching";

export interface ClaudeBilling {
  costMultiplier: string;
  modelSource: "request" | "response";
  dailyLimitUsd: string | null;
  monthlyLimitUsd: string | null;
}

export interface ClaudeTrafficSettings {
  restoreOnExit: boolean;
  headersTimeoutSeconds: number;
  streamingFirstByteTimeoutSeconds: number;
  streamingIdleTimeoutSeconds: number;
  nonStreamingTimeoutSeconds: number;
  circuitFailureThreshold: number;
  circuitSuccessThreshold: number;
  circuitCooldownSeconds: number;
  circuitErrorRatePercent: number;
  circuitMinRequests: number;
}

export interface ClaudeFailoverPolicy {
  enabled: boolean;
  providerIds: string[];
  maxRetries: number;
  takeover: boolean;
  traffic: ClaudeTrafficSettings;
}

export interface ClaudeFailoverProvider {
  id: string;
  name: string;
  fileHash: string;
  inQueue: boolean;
  routeable: boolean;
  health: ClaudeProviderHealth;
}

export interface ClaudeFailoverView {
  policy: ClaudeFailoverPolicy;
  providers: ClaudeFailoverProvider[];
  warnings: string[];
}

export function getClaudeFailover(): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("get_claude_failover");
}

export function setClaudeFailoverPolicy(
  policy: ClaudeFailoverPolicy,
  confirmWrite: boolean,
): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("set_claude_failover_policy", {
    policy,
    confirmWrite,
  });
}

export function setClaudeFailoverEnabled(
  enabled: boolean,
  confirmWrite: boolean,
): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("set_claude_failover_enabled", {
    enabled,
    confirmWrite,
  });
}

export function addClaudeFailoverProvider(
  providerId: string,
  confirmWrite: boolean,
): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("add_claude_failover_provider", {
    providerId,
    confirmWrite,
  });
}

export function removeClaudeFailoverProvider(
  providerId: string,
  confirmWrite: boolean,
): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("remove_claude_failover_provider", {
    providerId,
    confirmWrite,
  });
}

export function reorderClaudeFailoverProviders(
  orderedIds: string[],
  confirmWrite: boolean,
): Promise<ClaudeFailoverView> {
  return invoke<ClaudeFailoverView>("reorder_claude_failover_providers", {
    orderedIds,
    confirmWrite,
  });
}


export interface ClaudeGatewayStopPreview {
  backupId: string;
  contentHash: string;
  renderedHash: string;
  target: string;
  changes: KeyChange[];
}
export interface ClaudeGatewayStopOutcome { backup: BackupRecord; finalHash: string; warnings: string[] }
export const previewClaudeGatewayStop = (): Promise<ClaudeGatewayStopPreview> => invoke("preview_claude_gateway_stop");
export const stopClaudeGateway = (preview: ClaudeGatewayStopPreview, confirmWrite: boolean): Promise<ClaudeGatewayStopOutcome> =>
  invoke("stop_claude_gateway", { preview, confirmWrite });

export interface ClaudeProviderHealth {
  state: "closed" | "open" | "halfOpen";
  consecutiveFailures: number;
  consecutiveSuccesses: number;
  totalRequests: number;
  failedRequests: number;
  openUntilMs: number | null;
}
export const resetClaudeProviderHealth = (providerId: string, confirmWrite: boolean): Promise<ClaudeFailoverView> =>
  invoke("reset_claude_provider_health", { providerId, confirmWrite });

export interface ClaudeFailoverQueueMember {
  sourceName: string;
  baseUrl: string | null;
  model: string | null;
  matchedProfileId: string | null;
  matchedProfileName: string | null;
}

export interface ClaudeFailoverSourceScan {
  found: boolean;
  sourceRevision: string;
  policyRevision: string;
  members: ClaudeFailoverQueueMember[];
  proposal: ClaudeFailoverPolicy;
  warnings: string[];
}

export const scanClaudeFailoverSource = (sourcePath: string): Promise<ClaudeFailoverSourceScan> =>
  invoke("scan_claude_failover_source", { sourcePath });

export interface ClaudeFailoverImportOutcome {
  view: ClaudeFailoverView;
  queued: number;
  warnings: string[];
}

export const importClaudeFailoverSource = (
  sourcePath: string,
  sourceRevision: string,
  expectedPolicyHash: string,
  confirmWrite: boolean,
): Promise<ClaudeFailoverImportOutcome> =>
  invoke("import_claude_failover_source", { sourcePath, sourceRevision, expectedPolicyHash, confirmWrite });
