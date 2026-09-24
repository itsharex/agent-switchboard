import type { LanguagePreference } from "../api/settings";
import type { ResolvedLanguage } from "./types";

/** Ordered locales of the surrounding webview, best first. Both windows
 * (main and tray) resolve through this so they always agree. */
export function systemLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  const languages = navigator.languages;
  if (languages && languages.length > 0) return [...languages];
  return navigator.language ? [navigator.language] : [];
}

/** Shared resolution rule: an explicit preference wins; "system" maps a
 * Chinese desktop locale to Simplified Chinese and every other locale to
 * English. Unresolvable environments fall back to English. */
export function resolveLanguage(
  preference: LanguagePreference,
  systemLocales: readonly string[],
): ResolvedLanguage {
  if (preference !== "system") return preference;
  return systemLocales[0]?.split(/[-_]/)[0].toLowerCase() === "zh" ? "zh-CN" : "en-US";
}
