import type { AppKind } from "../api/client";

/**
 * Single owner of the AppKind display name.
 *
 * Two forms exist on purpose, and both live here so no module re-derives one:
 * `clientName` is the short label a control has room for (cards, segments,
 * filters, pills), `clientFullName` is the product's own name in prose
 * ("Claude Code：一个历史目录不可读"). Having a second local `clientName` in
 * the session module return the long form silently made the same identifier
 * mean two different strings.
 */
export function clientName(app: AppKind): string {
  return app === "codex" ? "Codex" : "Claude";
}

export function clientFullName(app: AppKind): string {
  return app === "codex" ? "Codex" : "Claude Code";
}
