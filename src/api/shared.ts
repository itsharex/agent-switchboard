/* Client-boundary primitives shared by every api domain module. */

export type AppKind = "codex" | "claude";
export type RouteMode = "official" | "custom";
/** The provider endpoint's actual wire protocol. */
export type UpstreamProtocol = "responses" | "chatCompletions" | "anthropicMessages" | "geminiGenerateContent";
export type AuthenticationScheme = "bearer" | "xApiKey" | "xGoogApiKey";

export interface ResponsesOptions {
  requestMode: "standard" | "minimal";
}

export interface CodexModelSettings {
  contextWindow: number | null;
}

export interface ClaudeModelSettings {
  primaryOneM: boolean;
  haikuModel: string | null;
  haikuOneM?: boolean;
  sonnetModel: string | null;
  sonnetOneM: boolean;
  opusModel: string | null;
  opusOneM: boolean;
  availableModels: string[] | null;
  fableModel?: string | null;
  fableOneM?: boolean;
  subagentModel?: string | null;
  subagentOneM?: boolean;
  displayNames?: { haiku: string | null; sonnet: string | null; opus: string | null; fable: string | null } | null;
}

export type ModelOptions =
  | ({ kind: "codex" } & CodexModelSettings)
  | ({ kind: "claude" } & ClaudeModelSettings);

export interface CommandError {
  code: string;
  message: string;
  /** Catalog key for the app-owned explanation of the current language;
   * present for app-owned explanations. External diagnostics retain `message`, which
   * remains the scrubbed raw diagnostic and the fallback rendering. */
  messageKey?: string;
  /** Interpolation values for `messageKey` placeholders. */
  params?: Record<string, string | number>;
}
