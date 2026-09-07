import type { CodexServerOptions, SecretSlotView } from "../../../api/client";
import type { SelectOption } from "../../Select";

/** How one named secret-bearing position is being edited. `keep` only
 * exists for stored credentials the renderer cannot see; leaving it untouched
 * omits the position from the request so the stored value survives. */
export type SlotKind = "keep" | "plain" | "envRef" | "secret";

export interface SlotDraft {
  /** Stable row identity so removing one row never remaps another row's
   * controls onto different state. */
  id: number;
  name: string;
  kind: SlotKind;
  text: string;
  initial: SecretSlotView | null;
}

export interface BearerDraft {
  initial: SecretSlotView | null;
  kind: SlotKind | "none";
  text: string;
}

export function slotKindOptions(initial: SecretSlotView | null): SelectOption[] {
  const options: SelectOption[] = [];
  if (initial?.mode === "secretConfigured") {
    options.push({ value: "keep", label: "保持已存凭据" });
  }
  options.push(
    { value: "plain", label: "明文值" },
    { value: "envRef", label: "环境变量" },
    { value: "secret", label: "新凭据（存入系统凭据）" },
  );
  return options;
}

export function initialSlotDraft(slot: { name: string; value: SecretSlotView }, id: number): SlotDraft {
  if (slot.value.mode === "secretConfigured") {
    return { id, name: slot.name, kind: "keep", text: "", initial: slot.value };
  }
  if (slot.value.mode === "envRef") {
    return { id, name: slot.name, kind: "envRef", text: slot.value.name, initial: slot.value };
  }
  return { id, name: slot.name, kind: "plain", text: slot.value.value, initial: slot.value };
}

export function slotUnchanged(row: SlotDraft): boolean {
  if (row.initial === null) return false;
  if (row.kind === "keep") return row.initial.mode === "secretConfigured";
  if (row.kind === "plain") {
    return row.initial.mode === "plain" && row.initial.value === row.text;
  }
  if (row.kind === "envRef") {
    return row.initial.mode === "envRef" && row.initial.name === row.text;
  }
  return false;
}

export function parseArgs(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function optionsEqual(a: CodexServerOptions | null, b: CodexServerOptions | null): boolean {
  return (
    (a?.cwd ?? null) === (b?.cwd ?? null) &&
    (a?.startupTimeoutSec ?? null) === (b?.startupTimeoutSec ?? null) &&
    (a?.toolTimeoutSec ?? null) === (b?.toolTimeoutSec ?? null) &&
    (a?.required ?? null) === (b?.required ?? null)
  );
}
