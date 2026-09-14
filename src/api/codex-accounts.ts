import { invoke } from "./client";
export interface CodexAccountLoginStart { sessionId: string; userCode: string; verificationUrl: string }
import type { CodexOfficialQuota } from "./usage";

export type CodexAccountSelection = { kind: "native" } | { kind: "default" } | { kind: "account"; id: string };
export interface CodexAccountSummary {
  id: string; email: string | null; plan: string | null; accountLabel: string;
  isDefault: boolean; generation: number; expiresAt: number; boundProfileIds: string[]; nativeSyncPending: boolean; nativeSyncError: string | null;
}
export interface CodexAccountsView {
  revision: string; accounts: CodexAccountSummary[]; bindings: Record<string, CodexAccountSelection>;
}
export type CodexAccountLoginPoll = { phase: "pending" } | { phase: "completed"; accounts: CodexAccountsView } | { phase: "failed"; message: string };
export interface CodexAccountModels { accountId: string; warning: string | null; models: Array<{ id: string; displayName: string; contextWindow: number | null }> }
export interface CodexAccountQuota { accountId: string; quota: CodexOfficialQuota; warning: string | null }

export const listCodexAccounts = (): Promise<CodexAccountsView> => invoke("list_codex_accounts");
export const importCodexNativeAccount = (expectedRevision: string, confirmWrite: boolean): Promise<CodexAccountsView> =>
  invoke("import_codex_native_account", { expectedRevision, confirmWrite });
export const setCodexDefaultAccount = (accountId: string | null, expectedRevision: string): Promise<CodexAccountsView> =>
  invoke("set_codex_default_account", { accountId, expectedRevision });
export const setCodexAccountBinding = (profileId: string, selection: CodexAccountSelection, expectedRevision: string): Promise<CodexAccountsView> =>
  invoke("set_codex_account_binding", { profileId, selection, expectedRevision });
export const deleteCodexAccount = (accountId: string, expectedRevision: string, confirmWrite: boolean): Promise<CodexAccountsView> =>
  invoke("delete_codex_account", { accountId, expectedRevision, confirmWrite });
export const startCodexAccountLogin = (accountId: string | null = null): Promise<CodexAccountLoginStart> => invoke("start_codex_account_login", { accountId });
export const pollCodexAccountLogin = (sessionId: string): Promise<CodexAccountLoginPoll> => invoke("poll_codex_account_login", { sessionId });
export const cancelCodexAccountLogin = (sessionId: string): Promise<void> => invoke("cancel_codex_account_login", { sessionId });
export const getCodexAccountModels = (accountId: string | null = null): Promise<CodexAccountModels> => invoke("get_codex_account_models", { accountId });
export const getCodexAccountQuota = (accountId: string | null = null): Promise<CodexAccountQuota> => invoke("get_codex_account_quota", { accountId });

export interface CodexAuthPolicyView { policy: { version: 1; preserveOfficialLogin: boolean }; revision: string }
export const getCodexAuthPolicy = (): Promise<CodexAuthPolicyView> => invoke("get_codex_auth_policy");
export const setCodexAuthPolicy = (preserveOfficialLogin: boolean, expectedRevision: string): Promise<CodexAuthPolicyView> =>
  invoke("set_codex_auth_policy", { preserveOfficialLogin, expectedRevision });
