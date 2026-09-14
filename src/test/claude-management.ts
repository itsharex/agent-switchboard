import { providerParameters } from "./provider-parameters";
import type { ProviderRecord } from "../api/providers";
import type { ClaudeAccountsView, ClaudeLoginView } from "../api/claude-accounts";
import type { ClaudeFailoverView } from "../api/claude-gateway";
import type { ClaudePromptsView, ClaudePromptDraft } from "../api/claude-prompts";
import type { ClaudeRequestLedgerSummary } from "../api/claude-ledger";
export const record: ProviderRecord = { fileHash: "provider-r1", profile: {
  id: "provider-one", app: "claude", routeMode: "custom", name: "隔离 Claude 供应商", model: "fixture-model", baseUrl: "https://fixture.invalid/v1", apiKey: "fake-stored-key",
  upstreamProtocol: "chatCompletions", authentication: "bearer", responsesOptions: null, maxOutputTokens: null, modelOptions: { kind: "claude", primaryOneM: false, haikuModel: "fixture-haiku", haikuOneM: true, sonnetModel: null, sonnetOneM: false, opusModel: null, opusOneM: false, availableModels: null },
  parameters: providerParameters("claude"), websiteUrl: null, connection: {},
} };
export const accounts: ClaudeAccountsView = { fileHash: "accounts-r1", accounts: [{ id: "account-one", label: "本地 Claude 账号", provider: "github_copilot", isDefault: false, expiresAtMs: null, upstreamAccountId: null, githubDomain: null }] };
export const login: ClaudeLoginView = { sessionId: "claude-login", phase: "pending", userCode: "CLAUDE-CODE", verificationUrl: "https://github.com/login/device", intervalSeconds: 5, expiresAtMs: Date.now() + 300000, accountId: null };
export const policy: ClaudeFailoverView = { policy: { enabled: false, providerIds: [], maxRetries: 0, takeover: false, traffic: {
  restoreOnExit: true, headersTimeoutSeconds: 20, streamingFirstByteTimeoutSeconds: 60, streamingIdleTimeoutSeconds: 120, nonStreamingTimeoutSeconds: 600, circuitFailureThreshold: 3, circuitSuccessThreshold: 1, circuitCooldownSeconds: 30, circuitErrorRatePercent: 60, circuitMinRequests: 10,
} }, providers: [{ id: "provider-one", name: "隔离 Claude 供应商", fileHash: "provider-r1", inQueue: false, routeable: true, health: { state: "closed", consecutiveFailures: 0, consecutiveSuccesses: 0, totalRequests: 0, failedRequests: 0, openUntilMs: null } }], warnings: [] };
export const prompts: ClaudePromptsView = { fileHash: "prompts-r1", activePromptId: "prompt-one", prompts: [{ id: "prompt-one", draft: { name: "本地 Prompt", description: null, content: "Old instructions" } }], liveHash: "live-r1", externalChange: false, pendingContent: false, recoveryRequired: false };
export const promptPlan = { promptId: "prompt-one", fileHash: "prompts-r1", liveHash: "live-r1", liveExists: true, renderedHash: "live-next" };
export const stopPlan = { backupId: "backup-one", contentHash: "before", renderedHash: "after", target: "isolated/claude/settings.json", changes: [] };
export const summary: ClaudeRequestLedgerSummary = { totalRequests: 3, failedRequests: 1, inputTokens: null, outputTokens: null, cacheReadTokens: null, cacheCreationTokens: null, reasoningTokens: null, totalDurationMs: 20, averageDurationMs: 10, averageFirstByteLatencyMs: null, averageFirstTokenLatencyMs: null, estimatedCostUsd: "0.001", pricedRequests: 1, unpricedRequests: 2 };
export const filePreview = { contentHash: "config-before", renderedHash: "config-after", content: '{"env":{"ANTHROPIC_AUTH_TOKEN":"<redacted>"}}', preview: { app: "claude", target: "isolated/settings.json", changes: [], warnings: [], backupDir: "isolated/backups" } };
export function answer(command: string, args?: Record<string, unknown>): unknown {
  switch (command) {
    case "list_claude_presets": return [{ id: "preset-one", name: "离线 Claude 预设", websiteUrl: "https://fixture.invalid", apiKeyUrl: null, category: "custom", authentication: "api_key", upstreamProtocol: "chatCompletions", endpointCandidates: [], variables: {}, unavailableReason: null }];
    case "search_claude_profiles": return [record];
    case "get_claude_accounts": case "set_claude_default_account": case "save_claude_account": return accounts;
    case "remove_claude_account": return { fileHash: "accounts-r2", accounts: [] };
    case "start_claude_account_login": case "poll_claude_account_login": return login;
    case "cancel_claude_account_login": return null;
    case "get_claude_account_models": return [{ id: "claude-model", ownedBy: "fixture" }];
    case "get_claude_account_quota": return { provider: "github_copilot", accountId: "account-one", plan: "fixture-plan", windows: [], checkedAtMs: Date.now() };
    case "prepare_claude_preset": { const { id: _id, ...draft } = record.profile; return { draft: { ...draft, apiKey: args?.apiKey }, warnings: [] }; }
    case "prepare_profile_save": return { preparationId: "claude-save", kind: args?.profileId ? "saveAndApply" : "create", preview: args?.profileId ? filePreview : null };
    case "commit_profile_save": case "duplicate_claude_profile": return record;
    case "get_claude_failover": case "reset_claude_provider_health": return policy;
    case "set_claude_failover_policy": return { ...policy, policy: args?.policy };
    case "preview_claude_gateway_stop": return stopPlan;
    case "stop_claude_gateway": return { warnings: [] };
    case "list_claude_prompts": return prompts;
    case "save_claude_prompt": return { ...prompts, fileHash: "prompts-r2", pendingContent: true, prompts: [{ id: args?.promptId ?? "new-prompt", draft: args?.draft as ClaudePromptDraft }] };
    case "preview_claude_prompt": return { plan: promptPlan, before: "Old instructions", after: "New instructions" };
    case "activate_claude_prompt": return { ...prompts, fileHash: "prompts-r3", liveHash: "live-next" };
    case "recover_claude_prompt": case "reorder_claude_prompts": return prompts;
    case "scan_claude_prompt_source": return { sourceRevision: "source-r1", prompts: [{ sourceId: "source-one", draft: { name: "来源 Prompt", description: null, content: "Imported instructions" }, enabledInSource: true }] };
    case "import_claude_prompt_source": return { view: prompts, imported: 1, unchanged: 0, warnings: [] };
    case "get_global_prompt_document": return { app: "claude", fileName: "CLAUDE.md", content: "Live document", contentHash: "live-r1", exists: true };
    case "get_claude_request_ledger": return { entries: [], offset: 0, limit: 50, total: 3, hasMore: false };
    case "get_claude_request_ledger_summary": return summary;
    case "get_claude_price_book": return { fileHash: "prices-r1", book: { version: 1, models: {} } };
    case "set_claude_price_book": return { fileHash: "prices-r2", book: args?.book };
    default: throw new Error("Unmocked Claude command: " + command);
  }
}
