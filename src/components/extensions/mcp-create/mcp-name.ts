export function validateUniqueName(name: string, existing: readonly string[], initial?: string) {
  if (name !== initial && existing.includes(name)) throw new Error("服务名称已存在，请使用其他名称");
}

export function uniquePresetName(base: string, existing: readonly string[]): string {
  let name = base;
  let index = 1;
  while (existing.includes(name)) name = base + "-" + index++;
  return name;
}
