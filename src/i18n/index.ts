/** Interface-language foundation: resolution, React bindings, the typed
 * message catalog, and the non-hook translator. */
export { useI18n, type I18n, type TFunction } from "./context";
export { commandErrorText } from "./errors";
export { currentLanguage, applyLanguagePreference, tr } from "./current";
export { entryLanguage, formatMessage } from "./format";
export { messages, type MessageKey } from "./messages";
export { resolveLanguage, systemLanguages } from "./resolve";
export type { MessageEntry, MessageParams, ResolvedLanguage } from "./types";
