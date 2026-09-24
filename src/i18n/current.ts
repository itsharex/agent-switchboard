import { entryLanguage, formatMessage } from "./format.ts";
import { messages, type MessageKey } from "./messages.ts";
import { resolveLanguage, systemLanguages } from "./resolve.ts";
import type { MessageParams, ResolvedLanguage } from "./types";

import type { LanguagePreference } from "../api/settings";

let preference: LanguagePreference = "system";
let activeLanguage: ResolvedLanguage = resolveLanguage(preference, systemLanguages());
const listeners = new Set<() => void>();

/** Apply only accepted settings snapshots, before publishing them to React. */
export function applyLanguagePreference(next: LanguagePreference): void {
  preference = next;
  refreshLanguage();
}

function refreshLanguage(): void {
  const next = resolveLanguage(preference, systemLanguages());
  if (typeof document !== "undefined") document.documentElement.lang = next;
  if (next === activeLanguage) return;
  activeLanguage = next;
  for (const listener of listeners) listener();
}

export function subscribeLanguage(listener: () => void): () => void {
  if (listeners.size === 0) {
    window.addEventListener("languagechange", refreshLanguage);
    window.addEventListener("focus", refreshLanguage);
    document.addEventListener("visibilitychange", refreshLanguage);
  }
  listeners.add(listener);
  refreshLanguage();
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0) return;
    window.removeEventListener("languagechange", refreshLanguage);
    window.removeEventListener("focus", refreshLanguage);
    document.removeEventListener("visibilitychange", refreshLanguage);
  };
}

export function currentLanguage(): ResolvedLanguage {
  return activeLanguage;
}

/** Translates with the module-level language. React components prefer the
 * `t` from `useI18n()` so a language switch re-renders; this variant is for
 * helpers outside React (formatters, label maps, plain-TS modules). */
export function tr(key: MessageKey, params?: MessageParams): string {
  return formatMessage(entryLanguage(messages[key], activeLanguage), params);
}
