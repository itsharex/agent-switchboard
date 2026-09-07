import { invoke } from "./client";
import type { AppKind } from "./shared";

/** Phases of one in-flight official login. */
export type OfficialLoginPhase = "pending" | "completed" | "failed";

/** Start payload: the device code to enter (Codex) or the authorize URL to
 * open (Claude). Never carries a credential. */
export interface OfficialLoginStart {
  userCode: string | null;
  verificationUrl: string;
}

/** Poll result: only status, codes, URLs, and fixed messages — no tokens. */
export interface OfficialLoginStatus {
  phase: OfficialLoginPhase;
  userCode: string | null;
  verificationUrl: string;
  message: string | null;
}

/** Starts the official login flow for one client; the previous session for
 * that client must be finished or cancelled first. */
export function startOfficialLogin(target: AppKind): Promise<OfficialLoginStart> {
  return invoke<OfficialLoginStart>("official_login_start", { target });
}

/** Advances one login by a single step and writes the client's native
 * credential cache once the vendor approves. */
export function pollOfficialLogin(target: AppKind): Promise<OfficialLoginStatus> {
  return invoke<OfficialLoginStatus>("official_login_poll", { target });
}

/** Cancels one in-flight official login; a no-op without a running session. */
export function cancelOfficialLogin(target: AppKind): Promise<void> {
  return invoke<void>("official_login_cancel", { target });
}
