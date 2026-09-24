import type { CommandError } from "../api/shared";
import type { LocalizedMessage } from "../api/switching";
import type { TFunction } from "./context";
import { messages, type MessageKey, type MessageParams } from "./messages.ts";

/** App-owned messages remain structured through state and async callbacks. */
export function uiMessage(messageKey: MessageKey, params?: MessageParams): CommandError {
  return { code: "ui-message", message: "", messageKey, params };
}

export function errorText(error: unknown, t: TFunction): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    if ("code" in error && "message" in error) return commandErrorText(error as CommandError, t);
    if ("message" in error && typeof error.message === "string") return error.message;
  }
  return t("common.operationFailed");
}

/** Renders one backend warning in the current language. Unknown keys fall
 * back to the scrubbed `text` so a message can never go blank. */
export function localizedMessageText(message: LocalizedMessage, t: TFunction): string {
  return commandErrorText({
    code: "localized-message", message: message.text,
    messageKey: message.key, params: message.params,
  }, t);
}

/** Resolves a backend catalog string (spec labels, groups, directory
 * titles) that may be a catalog key. Known keys render in the current
 * language; external diagnostics and user-provided values render verbatim. */
export function catalogText(text: string, t: TFunction): string {
  return text in messages ? t(text as keyof typeof messages) : text;
}

/**
 * Renders a command error in the current language. App-owned failures carry
 * `messageKey` (+ `params`); their scrubbed `message` stays available on the
 * error object as diagnostic detail. Unknown keys fall back to the raw
 * message, so an unmatched key can never blank an error.
 *
 * Convention: a parameter whose name ends in `Key` carries a catalog key
 * itself (e.g. `stageKey: "errors.switch.stage.backup"`) and is rendered
 * through the catalog; unknown nested keys render their raw value.
 */
export function commandErrorText(error: CommandError, t: TFunction): string {
  if (error.messageKey && error.messageKey in messages) {
    const raw = error.params;
    if (!raw || typeof raw !== "object") return t(error.messageKey as keyof typeof messages);
    const params: Record<string, string | number> = {};
    for (const [name, value] of Object.entries(raw as Record<string, unknown>)) {
      if (value === null || value === undefined) continue;
      params[name] =
        name.endsWith("Key") && typeof value === "string" && value in messages
          ? t(value as keyof typeof messages)
          : (value as string | number);
    }
    return t(error.messageKey as keyof typeof messages, params);
  }
  return error.message;
}
