import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getCodexMetering, getCodexRequestLedger, getCodexRequestSummary,
  repriceCodexRequests, setCodexMetering, type CodexMeteringSettings } from "./codex-metering";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });
it("uses only the Codex accounting commands and preserves filter boundaries", async () => {
  const filter = { profileId: "codex-p1", fromMs: 10, untilMs: 20 };
  await getCodexMetering(); await getCodexRequestLedger(filter, 20, 10); await getCodexRequestSummary(filter);
  expect(mocked.mock.calls).toEqual([["get_codex_metering"],
    ["get_codex_request_ledger", { filter, offset: 20, limit: 10 }], ["get_codex_request_summary", { filter }]]);
});
it("never invents confirmation or a settings revision for writes", async () => {
  const settings: CodexMeteringSettings = { version: 1, prices: {}, providers: {} };
  await setCodexMetering(settings, "revision", false); await repriceCodexRequests(null, "new-revision", true);
  expect(mocked.mock.calls).toEqual([
    ["set_codex_metering", { settings, expectedRevision: "revision", confirmWrite: false }],
    ["reprice_codex_requests", { filter: null, expectedRevision: "new-revision", confirmWrite: true }],
  ]);
});
