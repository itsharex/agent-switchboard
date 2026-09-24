import { tr } from "../i18n/current.ts";

/** Mirrors the native shortcut validator; the desktop remains authoritative. */
export function captureGlobalShortcut(event: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "metaKey" | "shiftKey">): string | null {
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
  if (!/^(Key[A-Z]|Digit[0-9]|Space|F([1-9]|1[0-2]))$/.test(event.code)) return null;
  return [event.shiftKey && "shift", event.ctrlKey && "control", event.altKey && "alt", event.metaKey && "super", event.code]
    .filter(Boolean).join("+");
}

export function globalShortcutLabel(chord: string): string {
  if (!chord) return tr("settings.shortcut.notSet");
  const labels: Record<string, string> = { shift: "Shift", control: "Ctrl", alt: "Alt", super: "⌘ / Win", Space: tr("settings.shortcut.space") };
  return chord.split("+").map((part) => labels[part] ?? part.replace(/^(Key|Digit)/, "")).join(" + ");
}
