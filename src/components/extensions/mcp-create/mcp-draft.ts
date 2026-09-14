import type { AppKind, ExtensionDraft, SecretValue } from "../../../api/client";
import { isCredentialLiteral, type CreateCredential, type CreateSource } from "./mcp-json";
import { parseCodexOptions } from "./codex-options";

export type PutSecret = (value: string, purpose: string) => Promise<string | null>;

function validateCredential(value: CreateCredential, field: string, header = false, original?: CreateCredential) {
  if (value.mode === "secretConfigured") {
    if (original?.mode !== "secretConfigured") throw new Error(field + " 无可保留凭据；请填写新值");
    return;
  }
  if (value.mode === "envRef") {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(value.name)) {
      throw new Error(`${field} 的环境变量引用名称不合法`);
    }
    return;
  }
  if (value.value.includes("\0") || (header && /[\r\n]/.test(value.value))) {
    throw new Error(`${field} 不能包含空字符或请求头换行`);
  }
  if (value.mode === "secret" && !value.value.trim()) throw new Error(`${field} 凭据不能为空`);
  if (value.mode === "plain" && isCredentialLiteral(value.value)) {
    throw new Error(`${field} 疑似凭据，请改用系统凭据类型`);
  }
}

function validateHeaders(headers: Record<string, CreateCredential>, original?: Record<string, CreateCredential>) {
  const seen = new Set<string>();
  for (const [name, value] of Object.entries(headers)) {
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name)) throw new Error(`请求头名称 ${name} 不合法`);
    if (seen.has(name.toLowerCase())) throw new Error(`请求头 ${name} 重复（名称不区分大小写）`);
    seen.add(name.toLowerCase());
    validateCredential(value, `请求头 ${name}`, true, original?.[name]);
  }
}

export function validateMcpSource({ name, server }: CreateSource, clients: AppKind[], original?: CreateSource) {
  if ((!original || name !== original.name) && !/^[A-Za-z0-9_-]{1,64}$/.test(name)) {
    throw new Error("服务名称须为 1–64 位字母、数字、下划线或连字符");
  }
  if (server.type === "stdio") {
    parseCodexOptions(server.codexOptions);
    if (!server.command.trim()) throw new Error("stdio 启动命令不能为空");
    if ([server.command, ...server.args].some((value) => value.includes("\0"))) {
      throw new Error("启动命令与参数不能包含空字符");
    }
    for (const [key, value] of Object.entries(server.env)) {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) throw new Error(`环境变量名 ${key} 不合法`);
      validateCredential(value, `环境变量 ${key}`, false, original?.server.type === server.type ? original.server.env[key] : undefined);
      if (clients.includes("codex") && value.mode === "envRef" && key !== value.name) {
        throw new Error(`Codex 不支持环境变量改名映射：${key} 必须与引用名 ${value.name} 相同`);
      }
    }
    return;
  }
  let url: URL;
  try { url = new URL(server.url); } catch { throw new Error("服务地址必须是有效的完整 URL"); }
  const protocols = server.type === "ws" ? ["ws:", "wss:", "http:", "https:"] : ["http:", "https:"];
  if (!protocols.includes(url.protocol)) throw new Error(`服务地址必须使用 ${protocols.join(" / ")}`);
  const initial = original?.server.type === server.type ? original.server : undefined;
  validateHeaders(server.headers, initial?.headers);
  if (server.type !== "http") {
    if (clients.includes("codex")) throw new Error("SSE / WebSocket 仅支持 Claude，请取消启用 Codex");
    return;
  }
  if (!server.bearer) return;
  validateCredential(server.bearer, "Bearer", true, initial?.type === "http" ? initial.bearer : undefined);
  if (Object.keys(server.headers).some((key) => key.toLowerCase() === "authorization")) {
    throw new Error("Bearer 与 Authorization 请求头不能同时设置");
  }
  if (clients.includes("codex") && server.bearer.mode !== "envRef") {
    throw new Error("Codex 的 Bearer 仅支持环境变量引用；请更改凭据类型或取消启用 Codex");
  }
}

export async function materializeCredential(value: CreateCredential, purpose: string, putSecret: PutSecret): Promise<SecretValue> {
  if (value.mode === "secretConfigured") throw new Error("已存凭据只能在原传输的原字段保留，不能复制到新配置");
  if (value.mode === "envRef") return { mode: "envRef", name: value.name };
  if (value.mode === "plain") return { mode: "plain", value: value.value };
  const reference = await putSecret(value.value, purpose);
  if (!reference?.trim()) throw new Error("凭据保存失败，请重试；服务尚未保存");
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
