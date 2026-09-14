import { beforeEach, expect, it, vi } from "vitest";
import { getClaudeAccounts, saveClaudeAccount, setClaudeDefaultAccount, removeClaudeAccount,
  startClaudeAccountLogin, pollClaudeAccountLogin, cancelClaudeAccountLogin, usesClaudeManagedAuth } from "./claude-accounts";
import { requiresGateway } from "../lib/protocol";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });

it("keeps account selection and device sessions on Claude-only commands with explicit write confirmation", async () => {
  await getClaudeAccounts();
  await setClaudeDefaultAccount("github_copilot", "42", "revision", true);
  await removeClaudeAccount("42", "revision", false);
  const request = { provider: "xai_oauth" as const, label: "Claude xAI", githubDomain: null, targetAccountId: null, makeDefault: true };
  await startClaudeAccountLogin(request, true);
  await pollClaudeAccountLogin("session");
  await cancelClaudeAccountLogin("session");
  expect(mocked.mock.calls).toEqual([
    ["get_claude_accounts"],
    ["set_claude_default_account", { provider: "github_copilot", accountId: "42", expectedFileHash: "revision", confirmWrite: true }],
    ["remove_claude_account", { accountId: "42", expectedFileHash: "revision", confirmWrite: false }],
    ["start_claude_account_login", { request, confirmWrite: true }],
    ["poll_claude_account_login", { sessionId: "session" }],
    ["cancel_claude_account_login", { sessionId: "session" }],
  ]);
});

it("uses a revision guarded credential import rather than writing either native client auth file", async () => {
  const account = { id: "42", label: "Claude Copilot", provider: "github_copilot" as const,
    accessToken: "isolated-token", refreshToken: null, expiresAtMs: null, upstreamAccountId: null, githubDomain: "github.com" };
  await saveClaudeAccount(account, "revision", true, true);
  expect(mocked).toHaveBeenCalledWith("save_claude_account", { account, expectedFileHash: "revision", makeDefault: true, confirmWrite: true });
});

it("routes keyless native-Anthropic managed profiles through Claude's gateway", () => {
  const connection = { providerType: "github_copilot" };
  expect(usesClaudeManagedAuth(connection)).toBe(true);
  expect(usesClaudeManagedAuth({})).toBe(false);
  expect(requiresGateway({ app: "claude", routeMode: "custom", upstreamProtocol: "anthropicMessages",
    responsesOptions: null, connection })).toBe(true);
});
