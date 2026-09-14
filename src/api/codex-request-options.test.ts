import { describe, expect, it } from "vitest";
import { reconcileCodexRequestOptions, type CodexRequestOptions } from "./codex-request-options";
const options: CodexRequestOptions = { promptCacheRouting: "enabled", anthropicCacheTtl: "1h", emulateClaudeCode: true };
describe("Codex request options", () => {
  it("clears only options belonging to a different protocol", () => {
    const connection = { customUserAgent: "custom", codex: options };
    expect(reconcileCodexRequestOptions(connection, "chatCompletions")).toEqual({ customUserAgent: "custom",
      codex: { promptCacheRouting: "enabled", anthropicCacheTtl: null, emulateClaudeCode: false } });
    expect(reconcileCodexRequestOptions(connection, "anthropicMessages").codex).toEqual({ ...options, promptCacheRouting: "auto" });
    expect(reconcileCodexRequestOptions(connection, "responses").codex).toEqual({
      promptCacheRouting: "auto", anthropicCacheTtl: null, emulateClaudeCode: false });
    expect(connection.codex).toBe(options);
  });
});
