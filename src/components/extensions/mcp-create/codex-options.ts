import type { CodexServerOptions } from "../../../api/client";

const NATIVE_OPTIONS = [
  ["cwd", "cwd"], ["startup_timeout_sec", "startupTimeoutSec"],
  ["tool_timeout_sec", "toolTimeoutSec"], ["required", "required"],
] as const;

export const NATIVE_CODEX_OPTION_FIELDS = NATIVE_OPTIONS.map(([name]) => name);

export function parseStdioOptions(spec: Record<string, unknown>): CodexServerOptions | undefined {
  const hasNative = NATIVE_CODEX_OPTION_FIELDS.some((key) => Object.hasOwn(spec, key));
  if (hasNative && Object.hasOwn(spec, "codexOptions")) {
    throw new Error("Codex 选项不能同时使用顶层原生字段与 codexOptions");
  }
  return parseCodexOptions(hasNative
    ? Object.fromEntries(NATIVE_OPTIONS.filter(([key]) => Object.hasOwn(spec, key)).map(([key, field]) => [field, spec[key]]))
    : spec.codexOptions);
}

export function parseCodexOptions(value: unknown): CodexServerOptions | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "object" || Array.isArray(value)) throw new Error("codexOptions 必须是 JSON 对象");
  const options = value as Record<string, unknown>;
  const unknown = Object.keys(options).filter((key) => !["cwd", "startupTimeoutSec", "toolTimeoutSec", "required"].includes(key));
  if (unknown.length) throw new Error("codexOptions 包含不支持的字段：" + unknown.join("、"));
  const result: CodexServerOptions = {};
  if (options.cwd != null) {
    if (typeof options.cwd !== "string" || !options.cwd.trim() || options.cwd.includes("\0")) {
      throw new Error("Codex 工作目录必须是非空字符串且不含空字符");
    }
    result.cwd = options.cwd;
  }
  for (const [key, label] of [["startupTimeoutSec", "启动超时"], ["toolTimeoutSec", "工具超时"]] as const) {
    if (options[key] == null) continue;
    if (typeof options[key] !== "number" || !Number.isSafeInteger(options[key]) || options[key] < 0) {
      throw new Error(label + "必须是非负安全整数秒");
    }
    result[key] = options[key];
  }
  if (options.required != null) {
    if (typeof options.required !== "boolean") throw new Error("Codex required 必须是布尔值");
    result.required = options.required;
  }
  return result;
}

export function codexOptionsEqual(a?: CodexServerOptions | null, b?: CodexServerOptions | null): boolean {
  return ["cwd", "startupTimeoutSec", "toolTimeoutSec", "required"].every((key) =>
    (a?.[key as keyof CodexServerOptions] ?? null) === (b?.[key as keyof CodexServerOptions] ?? null));
}
