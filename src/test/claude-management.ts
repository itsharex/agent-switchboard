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
  claudeFragment: { includeCoAuthoredBy: false, env: { CLAUDE_CODE_MAX_CONTEXT_TOKENS: "372000" } },
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
export const snippetScan = {
  found: true,
  sourceRevision: "snippet-source-r1",
  settingsRevision: "client-settings-r1",
  preview: {
    visual: [{ key: "spinnerTipsEnabled", value: "false" }],
    extraKeys: ["/env/DISABLE_TELEMETRY", "/permissions/allow"],
    rejected: ["env.ANTHROPIC_AUTH_TOKEN"],
  },
};
export const snippetImport = {
  snapshot: { settings: {}, settingsHash: "client-settings-r2" },
  visualChanged: 1,
  visualUnchanged: 0,
  extraChanged: 2,
  rejected: ["env.ANTHROPIC_AUTH_TOKEN"],
  warnings: ["只写入应用内的客户端偏好与通用片段；真实 Claude 配置仍由切换预览确认后应用", "未导入: env.ANTHROPIC_AUTH_TOKEN"],
};
export const failoverScan = {
  found: true,
  sourceRevision: "failover-source-r1",
  policyRevision: "policy-r1",
  members: [
    { sourceName: "中继 A", baseUrl: "https://relay.internal", model: "claude-x", matchedProfileId: "provider-one", matchedProfileName: "隔离 Claude 供应商" },
    { sourceName: "未导入中继", baseUrl: "https://other.fixture.invalid", model: null, matchedProfileId: null, matchedProfileName: null },
  ],
  proposal: policy.policy,
  warnings: ["未加入队列: 未导入中继（本地没有相同路由的档案，请先导入该供应商）"],
};
export const failoverImport = { view: policy, queued: 1, warnings: [] };
export const integration = { policy: { pluginIntegration: false }, flags: [
  { flag: "plugin", target: "isolated/claude/config.json", exists: false, applied: false, contentHash: "plugin-r1" },
  { flag: "onboarding", target: "isolated/.claude.json", exists: true, applied: true, contentHash: "onboarding-r1" },
] };
export const integrationPreview = { flag: "plugin", enable: true, target: "isolated/claude/config.json", contentHash: "plugin-r1", targetExisted: false, renderedHash: "plugin-r2",
  changes: [{ key: "primaryApiKey", kind: "set", before: null, after: '"any"' }] };
export const nativeQuota = { windows: [{ id: "five_hour", label: "5 小时", usedPercent: 12.5, resetsAtMs: null }], extraUsage: null, credentialExpiresAtMs: null, credentialExpired: false, checkedAtMs: Date.now() };
export const envScan = { revision: "env-r1", conflicts: [
  { varName: "ANTHROPIC_API_KEY", valuePreview: "••••••••", source: { kind: "file", path: "isolated/.zshrc", line: 3 } },
  { varName: "ANTHROPIC_BASE_URL", valuePreview: "https://relay.internal", source: { kind: "system", location: "HKEY_CURRENT_USER\\Environment" } },
] };
export const envBackup = { fileName: "env-20260914T000000.000Z.json", createdAt: "2026-09-14T00:00:00Z", entries: [envScan.conflicts[0]] };
export const sessionUsage = {
  report: { filesScanned: 2, imported: 3, updated: 0, gatewayMatched: 1, pinnedRewrites: 0, errors: [] },
  summary: { requests: 3, gatewayMatched: 1, inputTokens: 300, outputTokens: 45, cacheReadTokens: 20, cacheCreationTokens: 10, pricedRequests: 2, estimatedCostUsd: "0.001234",
    byModel: [{ model: "claude-sonnet-5", requests: 2, inputTokens: 200, outputTokens: 40, cacheReadTokens: 20, cacheCreationTokens: 10, pricedRequests: 2, estimatedCostUsd: "0.001234" },
      { model: "custom-model", requests: 1, inputTokens: 100, outputTokens: 5, cacheReadTokens: 0, cacheCreationTokens: 0, pricedRequests: 0, estimatedCostUsd: "0.000000" }],
    earliestAt: "2026-09-14T01:00:00.000Z", latestAt: "2026-09-14T02:00:00.000Z" },
};
export const endpoints = { providerId: "provider-one", fileHash: "provider-r1", endpoints: [{ url: "https://backup.fixture.invalid/v1", addedAt: 1, lastUsed: null }] };
export const endpointLatency = [
  { url: "https://fixture.invalid/v1", result: { grade: "ok", status: 200, latencyMs: 42, error: null, at: "2026-09-14T00:00:00Z" }, error: null },
  { url: "https://backup.fixture.invalid/v1", result: { grade: "unreachable", status: null, latencyMs: 5000, error: "连接失败", at: "2026-09-14T00:00:00Z" }, error: null },
];
export function answer(command: string, args?: Record<string, unknown>): unknown {
  switch (command) {
    case "list_provider_endpoints": return endpoints;
    case "add_provider_endpoint": return { ...endpoints, fileHash: "provider-r2", endpoints: [...endpoints.endpoints, { url: args?.url, addedAt: 2, lastUsed: null }] };
    case "remove_provider_endpoint": return { ...endpoints, fileHash: "provider-r2", endpoints: endpoints.endpoints.filter((endpoint) => endpoint.url !== args?.url) };
    case "test_claude_endpoints": return endpointLatency;
    case "scan_claude_mcp_source": return { sourceRevision: "mcp-source-r1", servers: [
      { sourceId: "m1", name: "docs", transport: "stdio", enabledForClaude: true, description: "文档", problem: null, existing: false },
      { sourceId: "m2", name: "events", transport: "http", enabledForClaude: false, description: null, problem: null, existing: true },
      { sourceId: "m3", name: "broken", transport: "unknown", enabledForClaude: true, description: null, problem: "server_config 不是 JSON 对象", existing: false },
    ] };
    case "import_claude_mcp_source": return { imported: ["docs"], unchanged: 0, warnings: ["只导入到扩展库；部署到 Claude 仍需在扩展工作区预览并确认"] };
    case "list_claude_presets": return [{ id: "preset-one", name: "离线 Claude 预设", websiteUrl: "https://fixture.invalid", apiKeyUrl: null, category: "custom", authentication: "api_key", upstreamProtocol: "chatCompletions", endpointCandidates: [], variables: {}, unavailableReason: null }];
    case "search_claude_profiles": return [record];
    case "get_claude_accounts": case "set_claude_default_account": case "save_claude_account": return accounts;
    case "remove_claude_account": return { fileHash: "accounts-r2", accounts: [] };
    case "start_claude_account_login": case "poll_claude_account_login": return login;
    case "cancel_claude_account_login": return null;
    case "get_claude_account_models": return [{ id: "claude-model", ownedBy: "fixture" }];
    case "get_claude_account_quota": return { provider: "github_copilot", accountId: "account-one", plan: "fixture-plan", windows: [], checkedAtMs: Date.now() };
    case "get_claude_native_quota": return nativeQuota;
    case "get_claude_integration": return integration;
    case "set_claude_integration_policy": return { ...integration, policy: args?.policy };
    case "preview_claude_integration": return { ...integrationPreview, flag: args?.flag, enable: args?.enable };
    case "apply_claude_integration": return { ...integration, flags: integration.flags.map((flag) => flag.flag === (args?.preview as { flag: string }).flag ? { ...flag, exists: true, applied: true, contentHash: "plugin-r2" } : flag) };
    case "scan_claude_env_conflicts": return envScan;
    case "remove_claude_env_conflicts": return envBackup;
    case "list_claude_env_backups": return [envBackup];
    case "restore_claude_env_backup": return 1;
    case "get_claude_session_usage": return sessionUsage;
    case "rebuild_claude_session_usage": return { ...sessionUsage, backupFile: "claude-session-usage-20260914T000000.000Z.json" };
    case "prepare_claude_preset": { const { id: _id, ...draft } = record.profile; return { draft: { ...draft, apiKey: args?.apiKey }, warnings: [] }; }
    case "prepare_profile_save": return { preparationId: "claude-save", kind: args?.profileId ? "saveAndApply" : "create", preview: args?.profileId ? filePreview : null };
    case "commit_profile_save": case "duplicate_claude_profile": return record;
    case "scan_claude_snippet_source": return snippetScan;
    case "import_claude_snippet_source": return snippetImport;
    case "scan_claude_failover_source": return failoverScan;
    case "import_claude_failover_source": return failoverImport;
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
