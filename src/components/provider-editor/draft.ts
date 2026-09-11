import type {
  AppKind,
  ClaudeModelSettings,
  ModelOptions,
  ProviderDraft,
  ProviderProfile,
  SettingsValues,
  UpstreamProtocol,
} from "../../api/client";
import { NATIVE_PROTOCOL } from "../../lib/protocol";
import { normalizeUsageQuery } from "../../lib/usage-query";

export type ProviderEditorDraft = Omit<ProviderDraft, "parameters"> & {
  parameters: SettingsValues | null;
};

/** What each wire format means for the endpoint the user is about to enter. */
export const PROTOCOL_NOTES: Record<UpstreamProtocol, string> = {
  responses: "按供应商要求填写完整 API 根地址（可含 /v1、/v2 或 /openai），不会自动补 /v1。例如 https://example.com/v1 → https://example.com/v1/responses。",
  chatCompletions: "按供应商要求填写完整 API 根地址（可含 /v1、/v2 或 /openai），不会自动补 /v1。例如 https://example.com/v2 → https://example.com/v2/chat/completions。",
  anthropicMessages: "填写供应商的服务根地址，请求在该地址后追加 /v1/messages。",
};

export const PROTOCOL_AUTHENTICATION_NOTES: Record<UpstreamProtocol, string> = {
  responses:
    "认证方式会自动使用 Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥；密钥值本身不变。",
  chatCompletions:
    "认证方式会自动使用 Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥；密钥值本身不变。",
  anthropicMessages:
    "认证方式会自动使用 x-api-key：以 x-api-key: <API 密钥> 请求头发送密钥；密钥值本身不变。",
};

export function defaultConnection(app: AppKind): Pick<ProviderDraft,
  "upstreamProtocol" | "maxOutputTokens" | "responsesOptions"> {
  return {
    upstreamProtocol: NATIVE_PROTOCOL[app],
    maxOutputTokens: null,
    responsesOptions: null,
  };
}

export function draftFrom(profile: ProviderProfile | null, initialApp: AppKind): ProviderEditorDraft {
  if (profile) {
    return {
      app: profile.app,
      routeMode: profile.routeMode,
      name: profile.name,
      model: profile.model,
      baseUrl: profile.baseUrl,
      apiKey: profile.apiKey,
      upstreamProtocol: profile.upstreamProtocol,
      responsesOptions: profile.responsesOptions,
      maxOutputTokens: profile.maxOutputTokens,
      modelOptions: profile.modelOptions,
      parameters: { settings: { ...profile.parameters.settings } },
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
    parameters: null,
    notes: null,
    websiteUrl: null,
    usageQuery: null,
    officialQuotaRefreshIntervalMinutes: null,
  };
}

export function prepareDraft(draft: ProviderEditorDraft): ProviderDraft | null {
  if (!draft.parameters || !responsesOptionsValid(draft)) return null;
  return {
    ...draft,
    parameters: draft.parameters,
    name: draft.name.trim(),
    model: optional(draft.model ?? ""),
    baseUrl: optional(draft.baseUrl ?? ""),
    apiKey: draft.apiKey.trim(),
    notes: optional(draft.notes ?? ""),
    websiteUrl: optional(draft.websiteUrl ?? ""),
    modelOptions: draft.modelOptions,
    usageQuery: normalizeUsageQuery(draft.usageQuery),
  };
}

export function responsesOptionsValid(draft: ProviderEditorDraft): boolean {
  if (draft.routeMode !== "custom" || draft.upstreamProtocol !== "responses") {
    return draft.responsesOptions === null;
  }
  const options = draft.responsesOptions;
  return Boolean(options && Object.keys(options).length === 1 && (
    options.requestMode === "standard" || options.requestMode === "minimal"
  ));
}

export function optional(value: string): string | null {
  const normalized = value.trim();
  return normalized || null;
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
