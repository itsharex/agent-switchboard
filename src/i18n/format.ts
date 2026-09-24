import type { MessageParams, ResolvedLanguage } from "./types";

/** Substitutes `{name}` placeholders. Unknown placeholders stay verbatim so a
 * missing parameter is visible in review instead of silently dropped. */
export function formatMessage(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : placeholder,
  );
}

const languageIndex: Record<ResolvedLanguage, 0 | 1> = { "zh-CN": 0, "en-US": 1 };

/** Picks the entry side for a language; used by the hook and the module-level
 * translator so both share one resolution path. */
export function entryLanguage(entry: readonly string[], language: ResolvedLanguage): string {
  return entry[languageIndex[language]];
}
