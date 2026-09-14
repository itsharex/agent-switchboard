import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { cancelCodexGatewayPolicy, commitCodexGatewayPolicy, discardCodexGatewayPolicy,
  getCodexGatewayPolicy, prepareCodexGatewayPolicy, recoverCodexGatewayPolicy, resetCodexProviderHealth,
  type CodexGatewayPolicy } from "./codex-gateway";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });
const policy: CodexGatewayPolicy = { version: 1, takeover: true, enabled: true,
  providerIds: ["primary", "backup"], maxRetries: 2,
  traffic: { headersTimeoutSeconds: 20, firstByteTimeoutSeconds: 30, idleTimeoutSeconds: 60,
    totalTimeoutSeconds: 600, failureThreshold: 3, cooldownSeconds: 30, successThreshold: 1,
    errorRatePercent: 60, minRequests: 10 } };
it("reads only Codex policy", async () => {
  await getCodexGatewayPolicy();
  expect(mocked).toHaveBeenCalledExactlyOnceWith("get_codex_gateway_policy");
});
it("previews policy without implicit confirmation", async () => {
  await prepareCodexGatewayPolicy("primary", policy);
  expect(mocked).toHaveBeenCalledExactlyOnceWith("prepare_codex_gateway_policy", { profileId: "primary", policy });
});
it("commits only the backend preparation and explicit confirmation", async () => {
  await commitCodexGatewayPolicy("prepared", true);
  expect(mocked).toHaveBeenCalledExactlyOnceWith("commit_codex_gateway_policy", { preparationId: "prepared", confirmWrite: true });
});
it("cancels a preparation without saving the policy", async () => {
  await cancelCodexGatewayPolicy("prepared");
  expect(mocked).toHaveBeenCalledExactlyOnceWith("cancel_codex_gateway_policy", { preparationId: "prepared" });
});
it("keeps recovery, discard and health reset distinct", async () => {
  await recoverCodexGatewayPolicy(); await discardCodexGatewayPolicy("live-hash", true);
  await resetCodexProviderHealth("primary", true);
  expect(mocked.mock.calls).toEqual([["recover_codex_gateway_policy"],
    ["discard_codex_gateway_policy", { expectedConfigHash: "live-hash", confirmWrite: true }],
    ["reset_codex_provider_health", { profileId: "primary", confirmWrite: true }]]);
});
