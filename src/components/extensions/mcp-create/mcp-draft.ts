import { uiMessage } from "../../../i18n/errors";
import type { AppKind, ExtensionDraft, SecretValue } from "../../../api/client";
import { tr } from "../../../i18n/current";
import { isCredentialLiteral, type CreateCredential, type CreateSource } from "./mcp-json";
import { parseCodexOptions } from "./codex-options";

export type PutSecret = (value: string, purpose: string) => Promise<string | null>;

function validateCredential(value: CreateCredential, field: string, header = false, original?: CreateCredential) {
  if (value.mode === "secretConfigured") {
    if (original?.mode !== "secretConfigured") throw uiMessage("mcp.error.noStoredCredential", { field });
    return;
  }
  if (value.mode === "envRef") {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(value.name)) {
      throw uiMessage("mcp.error.envRefInvalid", { field });
    }
    return;
  }
  if (value.value.includes("\0") || (header && /[\r\n]/.test(value.value))) {
    throw uiMessage("mcp.error.controlChars", { field });
  }
  if (value.mode === "secret" && !value.value.trim()) throw uiMessage("mcp.error.credentialEmpty", { field });
  if (value.mode === "plain" && isCredentialLiteral(value.value)) {
    throw uiMessage("mcp.error.looksLikeCredential", { field });
  }
}

function validateHeaders(headers: Record<string, CreateCredential>, original?: Record<string, CreateCredential>) {
  const seen = new Set<string>();
  for (const [name, value] of Object.entries(headers)) {
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name)) throw uiMessage("mcp.error.headerNameInvalid", { name });
    if (seen.has(name.toLowerCase())) throw uiMessage("mcp.error.headerDuplicate", { name });
    seen.add(name.toLowerCase());
    validateCredential(value, tr("mcp.error.fieldHeader", { name }), true, original?.[name]);
  }
}

export function validateMcpSource({ name, server }: CreateSource, clients: AppKind[], original?: CreateSource) {
  if ((!original || name !== original.name) && !/^[A-Za-z0-9_-]{1,64}$/.test(name)) {
    throw uiMessage("mcp.error.nameInvalid");
  }
  if (server.type === "stdio") {
    parseCodexOptions(server.codexOptions);
    if (!server.command.trim()) throw uiMessage("mcp.error.commandRequired");
    if ([server.command, ...server.args].some((value) => value.includes("\0"))) {
      throw uiMessage("mcp.error.commandNullChar");
    }
    for (const [key, value] of Object.entries(server.env)) {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) throw uiMessage("mcp.error.envNameInvalid", { name: key });
      validateCredential(value, tr("mcp.error.fieldEnv", { name: key }), false, original?.server.type === server.type ? original.server.env[key] : undefined);
      if (clients.includes("codex") && value.mode === "envRef" && key !== value.name) {
        throw uiMessage("mcp.error.envRenameUnsupported", { key, name: value.name });
      }
    }
    return;
  }
  let url: URL;
  try { url = new URL(server.url); } catch { throw uiMessage("mcp.error.urlInvalid"); }
  const protocols = server.type === "ws" ? ["ws:", "wss:", "http:", "https:"] : ["http:", "https:"];
  if (!protocols.includes(url.protocol)) throw uiMessage("mcp.error.urlProtocol", { protocols: protocols.join(" / ") });
  const initial = original?.server.type === server.type ? original.server : undefined;
  validateHeaders(server.headers, initial?.headers);
  if (server.type !== "http") {
    if (clients.includes("codex")) throw uiMessage("mcp.error.transportClaudeOnly");
    return;
  }
  if (!server.bearer) return;
  validateCredential(server.bearer, "Bearer", true, initial?.type === "http" ? initial.bearer : undefined);
  if (Object.keys(server.headers).some((key) => key.toLowerCase() === "authorization")) {
    throw uiMessage("mcp.error.bearerConflict");
  }
  if (clients.includes("codex") && server.bearer.mode !== "envRef") {
    throw uiMessage("mcp.error.bearerEnvRefOnly");
  }
}

export async function materializeCredential(value: CreateCredential, purpose: string, putSecret: PutSecret): Promise<SecretValue> {
  if (value.mode === "secretConfigured") throw uiMessage("mcp.error.storedCredentialCopy");
  if (value.mode === "envRef") return { mode: "envRef", name: value.name };
  if (value.mode === "plain") return { mode: "plain", value: value.value };
  const reference = await putSecret(value.value, purpose);
  if (!reference?.trim()) throw uiMessage("mcp.error.secretSaveFailed");
  return { mode: "secretRef", reference };
}

async function materializeMap(values: Record<string, CreateCredential>, name: string, putSecret: PutSecret) {
  const entries: [string, SecretValue][] = [];
  for (const [key, value] of Object.entries(values)) {
    entries.push([key, await materializeCredential(value, `mcp:${name}:${key}`, putSecret)]);
  }
  return Object.fromEntries(entries);
}

export async function materializeMcpDraft({ name, server }: CreateSource, putSecret: PutSecret): Promise<ExtensionDraft> {
  if (server.type === "stdio") {
    return { name, payload: {
      kind: "mcp", transport: "stdio", command: server.command, args: server.args,
      env: await materializeMap(server.env, name, putSecret),
      ...(server.codexOptions ? { codexOptions: server.codexOptions } : {}),
    } };
  }
  const headers = await materializeMap(server.headers, name, putSecret);
  if (server.type !== "http") {
    return { name, payload: {
      kind: "mcp", transport: server.type === "sse" ? "claudeSse" : "claudeWs", url: server.url, headers,
    } };
  }
  const bearer = server.bearer
    ? await materializeCredential(server.bearer, `mcp:${name}:bearer`, putSecret) : undefined;
  return { name, payload: { kind: "mcp", transport: "http", url: server.url, headers, ...(bearer ? { bearer } : {}) } };
}
