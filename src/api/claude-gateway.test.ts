import { beforeEach, describe, expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn().mockResolvedValue({}));
vi.mock("./client", () => ({ invoke }));
import { previewClaudeGatewayStop, stopClaudeGateway, setClaudeFailoverPolicy, type ClaudeFailoverPolicy } from "./claude-gateway";
import { getClaudeRequestLedger, getClaudeRequestLedgerSummary, setClaudePriceBook } from "./claude-ledger";

describe("Claude-only API contracts", () => {
  beforeEach(() => invoke.mockClear());
  it("requires the complete confirmed stop preview", async () => {
    await previewClaudeGatewayStop();
    const preview = { backupId:"backup", contentHash:"before", renderedHash:"after", target:"isolated", changes:[] };
    await stopClaudeGateway(preview, true);
    expect(invoke).toHaveBeenLastCalledWith("stop_claude_gateway", { preview, confirmWrite:true });
  });
  it("shares request history filters without mixing client-session usage", async () => {
    const filter = { profileId:"claude-provider", failuresOnly:true };
    await getClaudeRequestLedger(20, 10, filter);
    expect(invoke).toHaveBeenLastCalledWith("get_claude_request_ledger", { offset:20, limit:10, filter });
    await getClaudeRequestLedgerSummary(filter);
    expect(invoke).toHaveBeenLastCalledWith("get_claude_request_ledger_summary", { filter });
  });
  it("binds local price mutations to the file revision and confirmation", async () => {
    const book = { version:1 as const, models:{} };
    await setClaudePriceBook(book, "revision", true);
    expect(invoke).toHaveBeenLastCalledWith("set_claude_price_book", { book, expectedFileHash:"revision", confirmWrite:true });
  });
  it("keeps native takeover independent of enabling failover", async () => {
    const policy: ClaudeFailoverPolicy = { enabled:false, providerIds:[], maxRetries:2, takeover:true, traffic:{restoreOnExit:true,headersTimeoutSeconds:20,streamingFirstByteTimeoutSeconds:60,streamingIdleTimeoutSeconds:120,nonStreamingTimeoutSeconds:600,circuitFailureThreshold:3,circuitSuccessThreshold:2,circuitCooldownSeconds:30,circuitErrorRatePercent:60,circuitMinRequests:10} };
    await setClaudeFailoverPolicy(policy, true);
    expect(invoke).toHaveBeenLastCalledWith("set_claude_failover_policy", { policy, confirmWrite:true });
  });
});
