import { uiMessage } from "../../../i18n/errors";
import type { CreateCredential, CreateServer, CreateSource, McpType } from "./mcp-json";
import type { CodexServerOptions } from "../../../api/client";
import type { MessageKey } from "../../../i18n/messages";

export interface ArgumentRow { id: number; value: string }
export interface CredentialRow { id: number; name: string; value: CreateCredential; storedName?: string }

export type WizardServer =
  | { type: "stdio"; command: string; args: ArgumentRow[]; env: CredentialRow[]; codexOptions?: CodexServerOptions }
  | { type: "http"; url: string; headers: CredentialRow[]; bearer?: CreateCredential }
  | { type: "sse" | "ws"; url: string; headers: CredentialRow[] };

function credentialRows(values: Record<string, CreateCredential>): CredentialRow[] {
  return Object.entries(values).map(([name, value], index) => ({ id: index + 1, name, value: { ...value },
    ...(value.mode === "secretConfigured" ? { storedName: name } : {}) }));
}

export function initializeWizard(server: CreateServer): WizardServer {
  if (server.type === "stdio") {
    return {
      ...server, args: server.args.map((value, index) => ({ id: index + 1, value })),
      env: credentialRows(server.env),
    };
  }
  return { ...server, headers: credentialRows(server.headers) };
}

function credentialsMap(rows: CredentialRow[], labelKey: MessageKey) {
  const seen = new Set<string>();
  for (const row of rows) {
    if (!row.name) throw uiMessage("mcp.error.slotNameRequired", { labelKey });
    if (seen.has(row.name)) throw uiMessage("mcp.error.slotNameDuplicate", { labelKey, name: row.name });
    seen.add(row.name);
  }
  return Object.fromEntries(rows.map(({ name, value }) => [name, value]));
}

export function wizardSource(name: string, server: WizardServer): CreateSource {
  if (server.type === "stdio") {
    return { name: name.trim(), server: {
      ...server, args: server.args.map(({ value }) => value), env: credentialsMap(server.env, "mcp.error.labelEnv"),
    } };
  }
  return { name: name.trim(), server: { ...server, headers: credentialsMap(server.headers, "mcp.error.labelHeaders") } };
}

export function changeTransport(server: WizardServer, type: McpType): WizardServer {
  if (type === server.type) return server;
  if (type === "stdio") return { type, command: "", args: [], env: [] };
  return {
    type, url: server.type === "stdio" ? "" : server.url,
    headers: server.type === "stdio" ? [] : server.headers.filter((row) => row.value.mode !== "secretConfigured")
      .map(({ id, name, value }) => ({ id, name, value })),
  };
}

export function transportDropsFields(server: WizardServer, type: McpType): boolean {
  if (server.type === type) return false;
  if (server.type === "stdio") return Boolean(server.command || server.args.length || server.env.length || server.codexOptions);
  if (type === "stdio") return Boolean(server.url || server.headers.length || (server.type === "http" && server.bearer));
  return server.headers.some((row) => row.value.mode === "secretConfigured") || (server.type === "http" && server.bearer !== undefined);
}

export function credentialText(value: CreateCredential): string {
  if (value.mode === "secretConfigured") return "";
  return value.mode === "envRef" ? value.name : value.value;
}

export function credentialValue(mode: CreateCredential["mode"], text: string): CreateCredential {
  if (mode === "secretConfigured") return { mode };
  return mode === "envRef" ? { mode, name: text } : { mode, value: text };
}
