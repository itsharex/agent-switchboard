import type {
  AppKind,
  CodexModelSettings,
  ClaudeModelSettings,
  ModelOptions,
  ProviderDraft,
  ProviderProfile,
  UpstreamProtocol,
} from "../../api/client";
import { NATIVE_PROTOCOL } from "../../lib/protocol";

export const CONTEXT_WINDOW_1M = 1_000_000;

/** What each wire format means for the endpoint the user is about to enter. */
export const PROTOCOL_NOTES: Record<UpstreamProtocol, string> = {
  responses: "上游使用 OpenAI Responses 接口（/v1/responses）。",
  chatCompletions: "上游使用 OpenAI 兼容的 Chat Completions 接口（/v1/chat/completions）。",
  anthropicMessages: "上游使用 Anthropic Messages 接口（/v1/messages）。",
};

export const PROTOCOL_AUTHENTICATION_NOTES: Record<UpstreamProtocol, string> = {
  responses:
    "认证方式会自动使用 Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥；密钥值本身不变。",
  chatCompletions:
    "认证方式会自动使用 Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥；密钥值本身不变。",
  anthropicMessages:
    "认证方式会自动使用 x-api-key：以 x-api-key: <API 密钥> 请求头发送密钥；密钥值本身不变。",
};

export function defaultConnection(app: AppKind): {
  upstreamProtocol: UpstreamProtocol;
  maxOutputTokens: number | null;
} {
  return {
    upstreamProtocol: NATIVE_PROTOCOL[app],
    maxOutputTokens: null,
  };
}

export function draftFrom(profile: ProviderProfile | null, initialApp: AppKind): ProviderDraft {
  if (profile) {
    return {
      app: profile.app,
      routeMode: profile.routeMode,
      name: profile.name,
      model: profile.model,
      baseUrl: profile.baseUrl,
      apiKey: profile.apiKey,
      upstreamProtocol: profile.upstreamProtocol,
      maxOutputTokens: profile.maxOutputTokens,
      modelOptions: profile.modelOptions,
      notes: profile.notes ?? null,
      websiteUrl: profile.websiteUrl,
      usageQuery: profile.usageQuery ?? null,
      officialQuotaRefreshIntervalMinutes:
        profile.officialQuotaRefreshIntervalMinutes ?? null,
    };
  }
  return {
    app: initialApp,
    routeMode: "custom",
    name: "",
    model: null,
    baseUrl: null,
    apiKey: "",
    ...defaultConnection(initialApp),
    modelOptions: null,
    notes: null,
    websiteUrl: null,
    usageQuery: null,
    officialQuotaRefreshIntervalMinutes: null,
  };
}

export function optional(value: string): string | null {
  const normalized = value.trim();
  return normalized || null;
}

export function codexOptions(
  current: ModelOptions | null,
  patch: Partial<CodexModelSettings>,
): ModelOptions {
  const base: CodexModelSettings =
    current?.kind === "codex" ? current : { contextWindow: null };
  return { kind: "codex", ...base, ...patch };
}

export function claudeOptions(
  current: ModelOptions | null,
  patch: Partial<ClaudeModelSettings>,
): ModelOptions {
  const base: ClaudeModelSettings =
    current?.kind === "claude"
      ? current
      : {
          primaryOneM: false,
          haikuModel: null,
          sonnetModel: null,
          sonnetOneM: false,
          opusModel: null,
          opusOneM: false,
          availableModels: null,
        };
  return { kind: "claude", ...base, ...patch };
}

export function codexOptionsAreEmpty(options: ModelOptions | null): boolean {
  return !options || (options.kind === "codex" && options.contextWindow === null);
}
