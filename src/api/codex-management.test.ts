import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { duplicateCodexProfile, listCodexPresets, prepareCodexPreset, searchCodexProfiles } from "./codex-management";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); });
it("lists local presets without credentials or an activation request", async () => {
  mocked.mockResolvedValue([]);
  expect(await listCodexPresets()).toEqual([]);
  expect(mocked).toHaveBeenCalledExactlyOnceWith("list_codex_presets");
});
it("prepares a typed preset without saving or switching", async () => {
  mocked.mockResolvedValue({ draft: {}, warnings: [] });
  await prepareCodexPreset("codex-preset-02", "fixture-key");
  expect(mocked).toHaveBeenCalledExactlyOnceWith("prepare_codex_preset", { presetId: "codex-preset-02", apiKey: "fixture-key" });
});
it("copies only the expected Codex revision under an optional new name", async () => {
  mocked.mockResolvedValue({});
  await duplicateCodexProfile("source", "revision", "副本");
  expect(mocked).toHaveBeenCalledExactlyOnceWith("duplicate_codex_profile", { profileId: "source", expectedFileHash: "revision", name: "副本" });
});
it("passes search text without moving credentials into the query", async () => {
  mocked.mockResolvedValue([]);
  await searchCodexProfiles("Kimi");
  expect(mocked).toHaveBeenCalledExactlyOnceWith("search_codex_profiles", { query: "Kimi" });
});
it("preserves stale-copy diagnostics", async () => {
  const error = { code: "codex-profile-copy-failed", message: "文件已变化" };
  mocked.mockRejectedValue(error);
  await expect(duplicateCodexProfile("source", "stale")).rejects.toBe(error);
});
