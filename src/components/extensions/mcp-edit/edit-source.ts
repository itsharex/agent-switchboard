import type { McpEditViewEnvelope, SecretSlot } from "../../../api/client";
import type { CreateCredential, CreateSource } from "../mcp-create/mcp-json";

function slots(values: SecretSlot[]): Record<string, CreateCredential> {
  return Object.fromEntries(values.map(({ name, value }) => [name, { ...value }]));
}

export function mcpEditSource(envelope: McpEditViewEnvelope): CreateSource {
  const { name } = envelope;
  if (envelope.transport === "stdio") {
    return { name, server: { type: "stdio", command: envelope.command, args: [...envelope.args], env: slots(envelope.env),
      ...(envelope.codexOptions ? { codexOptions: { ...envelope.codexOptions } } : {}) } };
  }
  if (envelope.transport === "http") {
    return { name, server: { type: "http", url: envelope.url, headers: slots(envelope.headers),
      ...(envelope.bearer ? { bearer: { ...envelope.bearer } } : {}) } };
  }
  return { name, server: { type: envelope.transport === "claudeSse" ? "sse" : "ws", url: envelope.url, headers: slots(envelope.headers) } };
}
