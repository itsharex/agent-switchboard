import { invoke } from "./client";
import type { FilePreview, SwitchOutcome } from "./switching";
export interface CodexTrafficSettings {
  headersTimeoutSeconds: number;
  firstByteTimeoutSeconds: number;
  idleTimeoutSeconds: number;
  totalTimeoutSeconds: number;
  /** Whole-response deadline for non-streaming requests; 0 means unlimited. */
  nonStreamingTimeoutSeconds: number;
  failureThreshold: number;
  cooldownSeconds: number;
  successThreshold: number;
  errorRatePercent: number;
  minRequests: number;
}
export interface CodexGatewayPolicy {
  version: 1;
  takeover: boolean;
  enabled: boolean;
  providerIds: string[];
  maxRetries: number;
  traffic: CodexTrafficSettings;
  mediaFallback: boolean;
}
export interface CodexPolicyView {
  policy: CodexGatewayPolicy;
  revision: string;
  activeProfileId: string | null;
  providers: Array<{ id: string; name: string; fileHash: string; inQueue: boolean;
    health: Array<{ endpoint: string; circuitState: "closed" | "open" | "halfOpen"; consecutiveFailures: number }> }>;
  pendingRecovery: boolean;
  warning: string | null;
}
export interface CodexPolicyPreparation { preparationId: string; profileId: string; preview: FilePreview }
export const getCodexGatewayPolicy = (): Promise<CodexPolicyView> => invoke("get_codex_gateway_policy");
export const prepareCodexGatewayPolicy = (profileId: string, policy: CodexGatewayPolicy): Promise<CodexPolicyPreparation> =>
  invoke("prepare_codex_gateway_policy", { profileId, policy });
export const commitCodexGatewayPolicy = (preparationId: string, confirmWrite: boolean): Promise<SwitchOutcome> =>
  invoke("commit_codex_gateway_policy", { preparationId, confirmWrite });
export const cancelCodexGatewayPolicy = (preparationId: string): Promise<void> =>
  invoke("cancel_codex_gateway_policy", { preparationId });
export const recoverCodexGatewayPolicy = (): Promise<CodexPolicyView> => invoke("recover_codex_gateway_policy");
export const discardCodexGatewayPolicy = (expectedConfigHash: string, confirmWrite: boolean): Promise<CodexPolicyView> =>
  invoke("discard_codex_gateway_policy", { expectedConfigHash, confirmWrite });
export const resetCodexProviderHealth = (profileId: string, confirmWrite: boolean): Promise<CodexPolicyView> =>
  invoke("reset_codex_provider_health", { profileId, confirmWrite });
export interface CodexFailoverQueueMember {
  sourceName: string;
  endpoint: string | null;
  upstream: "responses" | "chatCompletions" | "anthropicMessages" | null;
  matchedProfileId: string | null;
  matchedProfileName: string | null;
}
/** Read-only the source application scan; the proposal is applied through the normal preview/commit transaction. */
export interface CodexFailoverSourceScan {
  found: boolean;
  sourceRevision: string;
  policyRevision: string;
  members: CodexFailoverQueueMember[];
  proposal: CodexGatewayPolicy;
  warnings: string[];
}
export const scanCodexFailoverSource = (sourcePath: string): Promise<CodexFailoverSourceScan> =>
  invoke("scan_codex_failover_source", { sourcePath });
