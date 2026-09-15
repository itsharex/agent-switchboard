import { beforeEach, expect, it, vi } from "vitest";
import { applyClaudeIntegration, getClaudeIntegration, previewClaudeIntegration, setClaudeIntegrationPolicy } from "./claude-integration";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });

it("routes both Claude client markers through preview, explicit confirmation, and a Claude-only policy", async () => {
  const preview = { flag: "onboarding" as const, enable: false, target: "isolated/.claude.json", contentHash: "r1", targetExisted: true, renderedHash: "r2", changes: [] };
  await getClaudeIntegration();
  await previewClaudeIntegration("onboarding", false);
  await applyClaudeIntegration(preview, true);
  await setClaudeIntegrationPolicy({ pluginIntegration: true }, false);
  expect(mocked.mock.calls).toEqual([
    ["get_claude_integration"],
    ["preview_claude_integration", { flag: "onboarding", enable: false }],
    ["apply_claude_integration", { preview, confirmWrite: true }],
    ["set_claude_integration_policy", { policy: { pluginIntegration: true }, confirmWrite: false }],
  ]);
});
