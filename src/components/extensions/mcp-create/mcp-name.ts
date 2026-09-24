import { uiMessage } from "../../../i18n/errors";

export function validateUniqueName(name: string, existing: readonly string[], initial?: string) {
  if (name !== initial && existing.includes(name)) throw uiMessage("mcp.error.nameExists");
}

export function uniquePresetName(base: string, existing: readonly string[]): string {
  let name = base;
  let index = 1;
  while (existing.includes(name)) name = base + "-" + index++;
  return name;
}
