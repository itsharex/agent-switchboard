import type { CodexUpstream, ProviderConnectionOptions } from "./providers";
export interface CodexRequestOptions {
  promptCacheRouting: "auto" | "enabled" | "disabled";
  anthropicCacheTtl: "5m" | "1h" | null;
  emulateClaudeCode: boolean;
}
/** A deliberate protocol change clears options owned by the previous bridge. */
export function reconcileCodexRequestOptions(connection: ProviderConnectionOptions,
  upstream: CodexUpstream): ProviderConnectionOptions {
  if (!connection.codex) return connection;
  return { ...connection, codex: {
    promptCacheRouting: upstream === "chatCompletions" ? connection.codex.promptCacheRouting : "auto",
    anthropicCacheTtl: upstream === "anthropicMessages" ? connection.codex.anthropicCacheTtl : null,
    emulateClaudeCode: upstream === "anthropicMessages" && connection.codex.emulateClaudeCode,
  } };
}
