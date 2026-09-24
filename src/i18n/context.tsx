import { useMemo, useSyncExternalStore } from "react";
import { currentLanguage, subscribeLanguage } from "./current";
import { entryLanguage, formatMessage } from "./format";
import { messages, type MessageKey } from "./messages";
import type { MessageParams, ResolvedLanguage } from "./types";

export type TFunction = (key: MessageKey, params?: MessageParams) => string;
export interface I18n {
  language: ResolvedLanguage;
  t: TFunction;
}

/** Components and plain formatters read the same committed language snapshot. */
export function useI18n(): I18n {
  const language = useSyncExternalStore(subscribeLanguage, currentLanguage);
  return useMemo(() => ({
    language,
    t: (key: MessageKey, params?: MessageParams) =>
      formatMessage(entryLanguage(messages[key], language), params),
  }), [language]);
}
