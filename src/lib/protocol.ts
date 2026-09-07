import type { AppKind, UpstreamProtocol } from "../api/client";

/** TS mirror of the routing contract owned by asb-core
 * `UpstreamProtocol::native_for`: the wire protocol each client speaks
 * natively. The local gateway converts only when a provider's upstream
 * protocol differs from this. */
export const NATIVE_PROTOCOL: Record<AppKind, UpstreamProtocol> = {
  codex: "responses",
  claude: "anthropicMessages",
};

/** Single owner of the UpstreamProtocol display name across pages and forms. */
export const PROTOCOL_LABELS: Record<UpstreamProtocol, string> = {
  responses: "OpenAI Responses",
  chatCompletions: "Chat Completions",
  anthropicMessages: "Anthropic Messages",
};
