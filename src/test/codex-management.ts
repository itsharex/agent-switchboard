import type { CodexAccountsView } from "../api/codex-accounts";
import type { CodexPolicyView } from "../api/codex-gateway";
import type { CodexPromptsView } from "../api/codex-prompts";
export const filePreview = { preview: { app: "codex", target: "config.toml", changes: [], warnings: [], backupDir: "fixture" }, contentHash: "before", renderedHash: "after", content: "model_provider = 'openai'", authHash: "auth-before", authRenderedHash: "auth-after", authExisted: true };
export const accounts: CodexAccountsView = { revision: "accounts-r1", bindings: {}, accounts: [{ id: "account-one", email: "one@example.test", plan: "team", accountLabel: "digest", isDefault: false, generation: 1, expiresAt: 4102444800000, boundProfileIds: [], nativeSyncPending: false, nativeSyncError: null }] };
export const official = { profile: { id: "official", app: "codex", routeMode: "official", name: "Codex 官方登录" }, fileHash: "official-hash" };
export const provider = { profile: { id: "provider-one", name: "隔离供应商", defaultModel: "model-one" }, fileHash: "provider-hash" };
export const policy: CodexPolicyView = {
  policy: { version: 1, enabled: false, takeover: false, providerIds: [], maxRetries: 2, traffic: { headersTimeoutSeconds: 20, firstByteTimeoutSeconds: 30, idleTimeoutSeconds: 60, totalTimeoutSeconds: 600, failureThreshold: 3, cooldownSeconds: 30, successThreshold: 1, errorRatePercent: 60, minRequests: 10 } },
  revision: "policy-r1", activeProfileId: null, providers: [{ id: "provider-one", name: "隔离供应商", fileHash: "provider-hash", inQueue: false, health: [] }], pendingRecovery: false, warning: null,
};
export const prompts: CodexPromptsView = { revision: "prompts-r1", activeId: null, presets: [{ id: "prompt-one", draft: { name: "工作指令", description: null, content: "Be concise." }, createdAt: "2026-09-13T00:00:00Z", updatedAt: "2026-09-13T00:00:00Z" }], live: { app: "codex", fileName: "AGENTS.md", content: "Old instructions", contentHash: "prompt-before", exists: true }, pendingRecovery: false };
export function answer(command: string, args?: Record<string, unknown>): unknown {
  switch (command) {
    case "list_codex_presets": return [{ id: "preset", name: "离线预设", category: "third_party", websiteUrl: "https://example.test", apiKeyUrl: null, authentication: "apiKey", upstream: "responses", endpointCandidates: ["https://example.test/v1"], models: ["model-one"] }];
    case "search_codex_profiles": case "list_codex_profiles": return [provider];
    case "list_profiles": return [official];
    case "prepare_codex_preset": return { draft: { name: "离线预设" }, warnings: [] };
    case "create_codex_profile": case "duplicate_codex_profile": return provider;
    case "list_codex_accounts": return accounts;
    case "get_codex_auth_policy": case "set_codex_auth_policy": return { policy: { version: 1, preserveOfficialLogin: args?.preserveOfficialLogin ?? true }, revision: "auth-policy" };
    case "set_codex_account_binding": return { ...accounts, revision: "accounts-r2", bindings: { official: args?.selection } };
    case "set_codex_default_account": return { ...accounts, revision: "accounts-r2" };
    case "start_codex_account_login": return { sessionId: "login-session", userCode: "TEST-CODE", verificationUrl: "https://auth.openai.com/codex/device" };
    case "cancel_codex_account_login": return null;
    case "poll_codex_account_login": return { phase: "pending" };
    case "preview_switch": return filePreview;
    case "execute_switch": return {};
    case "get_codex_gateway_policy": case "reset_codex_provider_health": return policy;
    case "prepare_codex_gateway_policy": return { preparationId: "policy-preparation", profileId: "provider-one", preview: filePreview };
    case "commit_codex_gateway_policy": return {};
    case "cancel_codex_gateway_policy": return null;
    case "list_codex_prompts": case "save_codex_prompt": return prompts;
    case "preview_codex_prompt": return { plan: { presetId: args?.presetId, revision: "prompts-r1", liveHash: "prompt-before", renderedHash: "prompt-after" }, before: "Old instructions", after: "Be concise.", backfillsActive: false };
    case "apply_codex_prompt": return { ...prompts, activeId: "prompt-one" };
    case "get_codex_metering": return { settings: { version: 1, prices: {}, providers: {} }, revision: "prices-r1" };
    case "get_codex_request_ledger": return { records: [], total: 0 };
    case "get_codex_request_summary": return { requests: 1, failedRequests: 0, pricedRequests: 0, unpricedRequests: 1, inputTokens: 100, outputTokens: 20, cacheReadTokens: 0, cacheCreationTokens: 0, reasoningTokens: 0, estimatedUsd: "0", days: [] };
    default: throw Error("Unmocked command: " + command);
  }
}
