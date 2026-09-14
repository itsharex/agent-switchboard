import type { AppKind, ProviderDraft, UpstreamProtocol } from "../api/client";

/** TS mirror of the routing contract owned by asb-core
 * `UpstreamProtocol::native_for`: the wire protocol each client speaks
 * natively. */
export const NATIVE_PROTOCOL: Record<AppKind, UpstreamProtocol> = {
  codex: "responses",
  claude: "anthropicMessages",
};

/** Single owner of the UpstreamProtocol display name across pages and forms. */
export const PROTOCOL_LABELS: Record<UpstreamProtocol, string> = {
  responses: "OpenAI Responses",
  chatCompletions: "Chat Completions",
  anthropicMessages: "Anthropic Messages",
  geminiGenerateContent: "Gemini Native",
};

/** Mirrors asb-core's routing decision, including Responses field filtering. */
export function requiresGateway(profile: Pick<ProviderDraft,
  "app" | "routeMode" | "upstreamProtocol" | "responsesOptions" | "connection" | "authentication">): boolean {
  if (profile.app === "claude" && profile.connection?.claudeNative) return false;
  return profile.routeMode === "custom" && profile.upstreamProtocol !== null && (
    profile.upstreamProtocol !== NATIVE_PROTOCOL[profile.app]
    || (profile.upstreamProtocol === "responses" && profile.responsesOptions?.requestMode === "minimal")
    || (profile.app === "codex" && profile.authentication != null
      && profile.authentication !== "bearer")
    || (profile.app === "claude" && Boolean(profile.connection?.providerType
      || profile.connection?.authBinding?.source === "managed_account"))
    || Boolean(profile.connection?.claudeBilling)
    || Boolean(profile.connection?.claudePromptCacheKey)
    || Boolean(profile.connection?.isFullUrl)
    || Boolean(profile.connection?.customUserAgent?.trim())
    || Boolean(profile.connection?.localProxyRequestOverrides
      && (Object.keys(profile.connection.localProxyRequestOverrides.headers).length > 0
        || profile.connection.localProxyRequestOverrides.body !== null))
    || (profile.connection?.endpointAutoSelect !== false
      && Object.keys(profile.connection?.customEndpoints ?? {}).length > 0)
  );
}
