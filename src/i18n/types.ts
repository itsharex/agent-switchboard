/** A resolved display language; the "system" preference resolves at render time. */
export type ResolvedLanguage = "zh-CN" | "en-US";

/** One translated message written as a pair: [简体中文, English]. Both
 * languages live on the same entry so keys and edits stay in lockstep. */
export type MessageEntry = readonly [zh: string, en: string];

/** Interpolation values for `{name}` placeholders inside message templates. */
export type MessageParams = Record<string, string | number>;
