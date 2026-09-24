import { uiMessage } from "../i18n/errors";
import type { AppKind, SettingsValues, SettingValue } from "../api/client";
export type ClaudeExtraSettings = Record<string, unknown> | undefined;
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, canonical(item)]));
  return value;
}
export function sameClaudeExtra(left: ClaudeExtraSettings, right: ClaudeExtraSettings): boolean {
  return JSON.stringify(canonical(left ?? {})) === JSON.stringify(canonical(right ?? {}));
}
/** The shared UI transports these values; only Claude business logic owns them. */
export function clientSettingsPayload(app: AppKind, settings: Record<string, SettingValue>, extra: ClaudeExtraSettings): SettingsValues {
  if (!extra || Object.keys(extra).length === 0) return { settings };
  if (app !== "claude") throw uiMessage("clientConfig.error.claudeExtraCodex");
  return { settings, claudeExtra: extra };
}
