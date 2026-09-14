import type { AppKind, FieldEdit, McpEditRequest, McpEditViewEnvelope, McpFieldEdits, SecretValue } from "../../../api/client";
import { codexOptionsEqual } from "../mcp-create/codex-options";
import { materializeCredential, materializeMcpDraft, validateMcpSource, type PutSecret } from "../mcp-create/mcp-draft";
import type { CreateCredential, CreateServer, CreateSource } from "../mcp-create/mcp-json";
import { validateUniqueName } from "../mcp-create/mcp-name";
import { buildMetadata, metadataEqual, type MetadataDraft } from "../mcp-create/metadata";
import { mcpEditSource } from "./edit-source";

function sameCredential(a?: CreateCredential, b?: CreateCredential): boolean {
  if (!a || !b) return a === b;
  if (a.mode === "secretConfigured") return b.mode === a.mode;
  if (a.mode === "envRef") return b.mode === a.mode && a.name === b.name;
  return a.mode === b.mode && "value" in b && a.value === b.value;
}

async function slotEdits(current: Record<string, CreateCredential>, next: Record<string, CreateCredential>, name: string, put: PutSecret) {
  const changes: [string, FieldEdit<SecretValue>][] = [];
  for (const [key, value] of Object.entries(next)) {
    if (sameCredential(current[key], value)) continue;
    changes.push([key, { action: "replace", value: await materializeCredential(value, "mcp:" + name + ":" + key, put) }]);
  }
  for (const key of Object.keys(current)) {
    if (!Object.hasOwn(next, key)) changes.push([key, { action: "delete" }]);
  }
  return changes.length ? Object.fromEntries(changes) : undefined;
}

async function fieldEdits(current: CreateServer, next: CreateServer, name: string, put: PutSecret): Promise<McpFieldEdits> {
  const fields: McpFieldEdits = {};
  if (current.type === "stdio" && next.type === "stdio") {
    if (current.command !== next.command) fields.command = { action: "replace", value: next.command };
    if (JSON.stringify(current.args) !== JSON.stringify(next.args)) fields.args = { action: "replace", value: next.args };
    const env = await slotEdits(current.env, next.env, name, put);
    if (env) fields.env = env;
    if (!codexOptionsEqual(current.codexOptions, next.codexOptions)) {
      fields.codexOptions = next.codexOptions ? { action: "replace", value: next.codexOptions } : { action: "delete" };
    }
  } else if (current.type !== "stdio" && next.type !== "stdio") {
    if (current.url !== next.url) fields.url = { action: "replace", value: next.url };
    const headers = await slotEdits(current.headers, next.headers, name, put);
    if (headers) fields.headers = headers;
    if (current.type === "http" && next.type === "http" && !sameCredential(current.bearer, next.bearer)) {
      fields.bearer = next.bearer
        ? { action: "replace", value: await materializeCredential(next.bearer, "mcp:" + name + ":bearer", put) }
        : { action: "delete" };
    }
  }
  return fields;
}

export async function buildMcpEdit(
  envelope: McpEditViewEnvelope, source: CreateSource, metadata: MetadataDraft,
  clients: AppKind[], put: PutSecret, existingNames: string[] = [],
): Promise<McpEditRequest> {
  const original = mcpEditSource(envelope);
  validateUniqueName(source.name, existingNames, original.name);
  validateMcpSource(source, clients, original);
  const mcpMetadata = buildMetadata(metadata);
  const request: McpEditRequest = { expectedRevision: envelope.revision, fields: {} };
  if (source.name !== original.name) request.serverKey = source.name;
  if (!metadataEqual(envelope.mcpMetadata, mcpMetadata)) {
    request.mcpMetadata = mcpMetadata ? { action: "replace", value: mcpMetadata } : { action: "delete" };
  }
  if (source.server.type !== original.server.type) {
    const { payload } = await materializeMcpDraft(source, put);
    if (payload.kind !== "mcp") throw new Error("MCP 配置类型无效");
    const { kind: _kind, ...transport } = payload;
    request.transport = transport;
  } else {
    request.fields = await fieldEdits(original.server, source.server, source.name, put);
  }
  return request;
}
