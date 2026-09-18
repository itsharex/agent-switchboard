/**
 * Join class values, skipping falsy entries. Last value wins.
 */
export function cx(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}
