import { invoke } from "./client";

export type ClaudeAuthProvider = "github_copilot" | "codex_oauth" | "xai_oauth";

export interface ClaudeAccountView {
  id: string;
  label: string;
  provider: ClaudeAuthProvider;
  expiresAtMs: number | null;
  upstreamAccountId: string | null;
  githubDomain: string | null;
  isDefault: boolean;
}

export interface ClaudeAccountsView {
  fileHash: string;
  accounts: ClaudeAccountView[];
}

/** Credentials are accepted only on this explicit import/save boundary, never returned. */
export interface ClaudeAccountInput extends Omit<ClaudeAccountView, "isDefault"> {
  accessToken: string;
  refreshToken: string | null;
}

export interface ClaudeLoginRequest {
  provider: ClaudeAuthProvider;
  label: string;
  githubDomain: string | null;
  targetAccountId: string | null;
  makeDefault: boolean;
}

export interface ClaudeLoginView {
  sessionId: string;
  phase: "pending" | "completed";
  userCode: string;
  verificationUrl: string;
  intervalSeconds: number;
  expiresAtMs: number;
  accountId: string | null;
}

export const getClaudeAccounts = (): Promise<ClaudeAccountsView> => invoke("get_claude_accounts");

export const saveClaudeAccount = (
  account: ClaudeAccountInput, expectedFileHash: string, makeDefault: boolean, confirmWrite: boolean,
): Promise<ClaudeAccountsView> => invoke("save_claude_account", { account, expectedFileHash, makeDefault, confirmWrite });

export const setClaudeDefaultAccount = (
  provider: ClaudeAuthProvider, accountId: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudeAccountsView> => invoke("set_claude_default_account", { provider, accountId, expectedFileHash, confirmWrite });

export const removeClaudeAccount = (
  accountId: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<ClaudeAccountsView> => invoke("remove_claude_account", { accountId, expectedFileHash, confirmWrite });

export const startClaudeAccountLogin = (
  request: ClaudeLoginRequest, confirmWrite: boolean,
): Promise<ClaudeLoginView> => invoke("start_claude_account_login", { request, confirmWrite });

export const pollClaudeAccountLogin = (sessionId: string): Promise<ClaudeLoginView> =>
  invoke("poll_claude_account_login", { sessionId });

export const cancelClaudeAccountLogin = (sessionId: string): Promise<void> =>
  invoke("cancel_claude_account_login", { sessionId });

export function usesClaudeManagedAuth(connection?: import("./providers").ProviderConnectionOptions | null): boolean {
  return Boolean(connection?.providerType || connection?.authBinding?.source === "managed_account");
}

export interface ClaudeAccountQuota {
  provider: ClaudeAuthProvider;
  accountId: string;
  plan: string | null;
  windows: {
    id: string; label: string; usedPercent: number | null; remaining: number | null; limit: number | null;
    unlimited: boolean; unit: "requests" | "percent"; resetsAtMs: number | null;
  }[];
  checkedAtMs: number;
}
export const getClaudeAccountModels = (
  provider: ClaudeAuthProvider, accountId: string | null,
): Promise<import("./providers").ProviderModel[]> => invoke("get_claude_account_models", { provider, accountId });
export const getClaudeAccountQuota = (
  provider: ClaudeAuthProvider, accountId: string | null,
): Promise<ClaudeAccountQuota> => invoke("get_claude_account_quota", { provider, accountId });
