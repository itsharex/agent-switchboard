import { beforeEach, expect, it, vi } from "vitest";
import { activateClaudePrompt, importClaudePromptSource, listClaudePrompts, previewClaudePrompt,
  recoverClaudePrompt, saveClaudePrompt, scanClaudePromptSource } from "./claude-prompts";
import { listClaudePresets, prepareClaudePreset, duplicateClaudeProfile } from "./claude-providers";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });

it("keeps Claude preset preparation distinct from persistence and activation", async () => {
  await listClaudePresets();
  await prepareClaudePreset("claude-preset-80", "", {}, "42");
  await duplicateClaudeProfile("source", "副本", "revision", true);
  expect(mocked.mock.calls).toEqual([
    ["list_claude_presets"],
    ["prepare_claude_preset", { presetId: "claude-preset-80", apiKey: "", variables: {}, accountId: "42" }],
    ["duplicate_claude_profile", { profileId: "source", name: "副本", expectedFileHash: "revision", confirmWrite: true }],
  ]);
});

it("uses independent library and live-document revisions without accepting a native target path", async () => {
  const draft = { name: "工作", content: "先验证再改动", description: null };
  const plan = { promptId: "prompt", fileHash: "library", liveHash: "live", liveExists: true, renderedHash: "rendered" };
  await listClaudePrompts();
  await saveClaudePrompt(null, draft, "library", true);
  await previewClaudePrompt("prompt", "library");
  await activateClaudePrompt(plan, true);
  await recoverClaudePrompt(true);
  expect(mocked.mock.calls).toEqual([
    ["list_claude_prompts"],
    ["save_claude_prompt", { promptId: null, draft, expectedFileHash: "library", confirmWrite: true }],
    ["preview_claude_prompt", { promptId: "prompt", expectedFileHash: "library" }],
    ["activate_claude_prompt", { plan, confirmWrite: true }],
    ["recover_claude_prompt", { confirmWrite: true }],
  ]);
});

it("pins a read-only source scan and does not equate source enablement with a native write", async () => {
  await scanClaudePromptSource("isolated/source.db");
  await importClaudePromptSource("isolated/source.db", ["source-prompt"], "source-revision", "library", true);
  expect(mocked.mock.calls).toEqual([
    ["scan_claude_prompt_source", { sourcePath: "isolated/source.db" }],
    ["import_claude_prompt_source", { sourcePath: "isolated/source.db", sourceIds: ["source-prompt"], sourceRevision: "source-revision", expectedFileHash: "library", confirmWrite: true }],
  ]);
});
