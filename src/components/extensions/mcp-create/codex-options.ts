import { uiMessage } from "../../../i18n/errors";
import type { CodexServerOptions } from "../../../api/client";

const NATIVE_OPTIONS = [
  ["cwd", "cwd"], ["startup_timeout_sec", "startupTimeoutSec"],
  ["tool_timeout_sec", "toolTimeoutSec"], ["required", "required"],
] as const;

export const NATIVE_CODEX_OPTION_FIELDS = NATIVE_OPTIONS.map(([name]) => name);

export function parseStdioOptions(spec: Record<string, unknown>): CodexServerOptions | undefined {
  const hasNative = NATIVE_CODEX_OPTION_FIELDS.some((key) => Object.hasOwn(spec, key));
  if (hasNative && Object.hasOwn(spec, "codexOptions")) {
    throw uiMessage("mcp.error.codexDualForm");
  }
  return parseCodexOptions(hasNative
    ? Object.fromEntries(NATIVE_OPTIONS.filter(([key]) => Object.hasOwn(spec, key)).map(([key, field]) => [field, spec[key]]))
    : spec.codexOptions);
}

export function parseCodexOptions(value: unknown): CodexServerOptions | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "object" || Array.isArray(value)) throw uiMessage("mcp.error.codexNotObject");
  const options = value as Record<string, unknown>;
  const unknown = Object.keys(options).filter((key) => !["cwd", "startupTimeoutSec", "toolTimeoutSec", "required"].includes(key));
  if (unknown.length) throw uiMessage("mcp.error.codexUnknownFields", { fields: unknown.join(", ") });
  const result: CodexServerOptions = {};
  if (options.cwd != null) {
    if (typeof options.cwd !== "string" || !options.cwd.trim() || options.cwd.includes("\0")) {
      throw uiMessage("mcp.error.codexCwd");
    }
    result.cwd = options.cwd;
  }
  for (const [key, labelKey] of [["startupTimeoutSec", "mcp.error.labelStartupTimeout"], ["toolTimeoutSec", "mcp.error.labelToolTimeout"]] as const) {
    if (options[key] == null) continue;
    if (typeof options[key] !== "number" || !Number.isSafeInteger(options[key]) || options[key] < 0) {
      throw uiMessage("mcp.error.timeoutInvalid", { labelKey });
    }
    result[key] = options[key];
  }
  if (options.required != null) {
    if (typeof options.required !== "boolean") throw uiMessage("mcp.error.codexRequiredType");
    result.required = options.required;
  }
  return result;
}

export function codexOptionsEqual(a?: CodexServerOptions | null, b?: CodexServerOptions | null): boolean {
  return ["cwd", "startupTimeoutSec", "toolTimeoutSec", "required"].every((key) =>
    (a?.[key as keyof CodexServerOptions] ?? null) === (b?.[key as keyof CodexServerOptions] ?? null));
}
