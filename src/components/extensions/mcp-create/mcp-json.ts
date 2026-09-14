import type { CodexServerOptions } from "../../../api/client";
import { NATIVE_CODEX_OPTION_FIELDS, parseStdioOptions } from "./codex-options";

export type McpType = "stdio" | "http" | "sse" | "ws";

export type CreateCredential =
  | { mode: "plain" | "secret"; value: string }
  | { mode: "envRef"; name: string }
  | { mode: "secretConfigured" };

export type CreateServer =
  | { type: "stdio"; command: string; args: string[]; env: Record<string, CreateCredential>; codexOptions?: CodexServerOptions }
  | { type: "http"; url: string; headers: Record<string, CreateCredential>; bearer?: CreateCredential }
  | { type: "sse" | "ws"; url: string; headers: Record<string, CreateCredential> };

export interface CreateSource {
  name: string;
  server: CreateServer;
}

type JsonObject = Record<string, unknown>;

function object(value: unknown, field: string): JsonObject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${field} 必须是 JSON 对象`);
  }
  return value as JsonObject;
}

function fields(value: JsonObject, allowed: string[], field: string) {
  const unknown = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unknown.length) throw new Error(`${field} 包含不支持的字段：${unknown.join("、")}`);
}

function string(value: unknown, field: string): string {
  if (typeof value !== "string") throw new Error(`${field} 必须是字符串`);
  if (value.includes("\0")) throw new Error(`${field} 不能包含空字符`);
  return value;
}

// Matches the literal-value guard in asb-core/src/redact.rs before a draft is saved.
export function isCredentialLiteral(value: string): boolean {
  return /^(sk-|ghp_|gho_|github_pat_|xox|AKIA|AIza)/.test(value)
    || /^[A-Za-z0-9]{32,}$/.test(value);
}

export function parseCredential(value: unknown, field: string, stored = false): CreateCredential {
  if (typeof value === "string") {
    string(value, field);
    const variable = /^\$\{([A-Za-z_][A-Za-z0-9_]*)\}$/.exec(value);
    if (variable) return { mode: "envRef", name: variable[1] };
    const secret = Boolean(value.trim()) && (/token|secret|api[-_]?key|credential|auth|bearer/i.test(field)
      || isCredentialLiteral(value));
    return { mode: secret ? "secret" : "plain", value };
  }
  const entry = object(value, field);
  if (entry.mode === "secretConfigured" && stored) {
    fields(entry, ["mode"], field);
    return { mode: "secretConfigured" };
  }
  if (entry.mode === "envRef") {
    fields(entry, ["mode", "name"], field);
    return { mode: "envRef", name: string(entry.name, `${field}.name`) };
  }
  if (entry.mode === "plain" || entry.mode === "secret") {
    fields(entry, ["mode", "value"], field);
    return { mode: entry.mode, value: string(entry.value, `${field}.value`) };
  }
  if (["secretRef", "stored", "redacted", "secretConfigured"].includes(String(entry.mode))) {
    throw new Error(`${field} 不能导入已存凭据引用；请输入新凭据或环境变量引用`);
  }
  throw new Error(`${field} 值类型必须是 plain、envRef 或 secret`);
}

function credentials(value: unknown, field: string, initial?: Record<string, CreateCredential>): Record<string, CreateCredential> {
  if (value === undefined) return {};
  return Object.fromEntries(Object.entries(object(value, field))
    .map(([name, item]) => [name, parseCredential(item, `${field}.${name}`, initial?.[name]?.mode === "secretConfigured")]));
}

function argumentsList(value: unknown): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error("args 必须是字符串数组");
  return value.map((argument, index) => string(argument, `args[${index}]`));
}

function parseServer(value: unknown, original?: CreateServer): CreateServer {
  const spec = object(value, "MCP 服务配置");
  if (Object.hasOwn(spec, "command") && Object.hasOwn(spec, "url")) {
    throw new Error("command 与 url 不能同时出现");
  }
  const type = spec.type === undefined
    ? Object.hasOwn(spec, "command") ? "stdio" : Object.hasOwn(spec, "url") ? "http" : undefined
    : spec.type;
  if (type === "stdio") {
    fields(spec, ["type", "command", "args", "env", "codexOptions", ...NATIVE_CODEX_OPTION_FIELDS], "stdio 配置");
    const codexOptions = parseStdioOptions(spec);
    return {
      type, command: string(spec.command, "command"),
      args: argumentsList(spec.args),
      env: credentials(spec.env, "env", original?.type === type ? original.env : undefined),
      ...(codexOptions ? { codexOptions } : {}),
    };
  }
  if (type === "http" || type === "sse" || type === "ws") {
    fields(spec, ["type", "url", "headers", ...(type === "http" ? ["bearer"] : [])], `${type} 配置`);
    const initial = original?.type === type ? original : undefined;
    const remote = { url: string(spec.url, "url"), headers: credentials(spec.headers, "headers", initial?.headers) };
    if (type !== "http") return { type, ...remote };
    return {
      type, ...remote,
      ...(spec.bearer === undefined || spec.bearer === null
        ? {} : { bearer: parseCredential(spec.bearer, "bearer", initial?.type === "http" && initial.bearer?.mode === "secretConfigured") }),
    };
  }
  throw new Error("MCP type 仅支持 stdio、http、sse 或 ws；省略时必须提供 command 或 url");
}

function singleEntry(value: unknown, field: string) {
  const entries = Object.entries(object(value, field));
  if (entries.length !== 1) {
    throw new Error(`一次只能新建一个 MCP 服务，${field} 中检测到 ${entries.length} 项`);
  }
  return { name: entries[0][0], spec: object(entries[0][1], `${field} 服务条目`) };
}

function syntaxError(text: string, error: unknown): Error {
  const position = error instanceof Error ? /position (\d+)/.exec(error.message) : null;
  const lines = position ? text.slice(0, Number(position[1])).split(/\r\n|\n|\r/) : null;
  const location = lines ? `（第 ${lines.length} 行，第 ${lines[lines.length - 1].length + 1} 列）` : "";
  return new Error(`JSON 语法错误${location}，请检查引号、逗号与括号`);
}

function readEntry(text: string): { name?: string; spec: JsonObject; wrapper?: "mcpServers" | "named" } {
  if (!text.trim()) throw new Error("JSON 配置不能为空");
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (error) {
    throw syntaxError(text, error);
  }
  const root = object(value, "MCP JSON");
  if (Object.hasOwn(root, "mcpServers")) {
    fields(root, ["mcpServers"], "MCP JSON");
    return { ...singleEntry(root.mcpServers, "mcpServers"), wrapper: "mcpServers" };
  }
  if (["command", "url", "type"].some((key) => Object.hasOwn(root, key))) return { spec: root };
  return { ...singleEntry(root, "命名服务对象"), wrapper: "named" };
}

export function parseMcpJson(text: string, name = "", original?: CreateServer): CreateSource {
  const entry = readEntry(text);
  return { name: (entry.name ?? name).trim(), server: parseServer(entry.spec, original) };
}

export function jsonServiceName(text: string): string | undefined {
  try { return readEntry(text).name; } catch { return undefined; }
}

export function renameJsonService(text: string, name: string): string {
  try {
    const entry = readEntry(text);
    if (!entry.wrapper) return text;
    const named = { [name]: entry.spec };
    return JSON.stringify(entry.wrapper === "mcpServers" ? { mcpServers: named } : named, null, 2);
  } catch { return text; }
}

export function emptyServer(type: McpType): CreateServer {
  return type === "stdio"
    ? { type, command: "", args: [], env: {} }
    : { type, url: "", headers: {} };
}

export function formatMcpJson(server: CreateServer): string {
  return JSON.stringify(server, null, 2);
}
