/* Client-boundary primitives shared by every api domain module. */

export type AppKind = "codex" | "claude";
export type RouteMode = "official" | "custom";
/** The provider endpoint's actual wire protocol. The local gateway converts
 * only when it differs from the selected client's native protocol. */
export type UpstreamProtocol = "responses" | "chatCompletions" | "anthropicMessages";

export interface CodexModelSettings {
  contextWindow: number | null;
}

export interface ClaudeModelSettings {
  primaryOneM: boolean;
  haikuModel: string | null;
  sonnetModel: string | null;
  sonnetOneM: boolean;
  opusModel: string | null;
  opusOneM: boolean;
  availableModels: string[] | null;
}

export type ModelOptions =
  | ({ kind: "codex" } & CodexModelSettings)
  | ({ kind: "claude" } & ClaudeModelSettings);

export interface CommandError {
  code: string;
  message: string;
}
