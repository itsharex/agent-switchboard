import { invoke } from "./client";

/** One logged-in xAI (SuperGrok) account; tokens never leave the backend. */
export interface XaiAccountView {
  id: string;
  label: string;
  isDefault: boolean;
  expiresAtMs: number;
  boundProfileIds: string[];
}

export interface XaiAccountsView {
  revision: string;
  defaultAccountId: string | null;
  accounts: XaiAccountView[];
  bindings: Record<string, string>;
}

export interface XaiLoginSession {
  userCode: string;
  verificationUri: string;
  expiresAtMs: number;
}

export const listXaiAccounts = (): Promise<XaiAccountsView> =>
  invoke("list_xai_accounts", {});

export const startXaiLogin = (): Promise<XaiLoginSession> =>
  invoke("start_xai_login", {});

export const pollXaiLogin = (): Promise<{ status: "pending" | "completed" }> =>
  invoke("poll_xai_login", {});

export const cancelXaiLogin = (): Promise<boolean> =>
  invoke("cancel_xai_login", {});

export const deleteXaiAccount = (
  accountId: string, expectedRevision: string,
): Promise<XaiAccountsView> =>
  invoke("delete_xai_account", { accountId, expectedRevision });

/** Binds (or unbinds with null) one Codex profile to one xAI account. */
export const setXaiAccountBinding = (
  profileId: string, accountId: string | null, expectedRevision: string,
): Promise<XaiAccountsView> =>
  invoke("set_xai_account_binding", { profileId, accountId, expectedRevision });
