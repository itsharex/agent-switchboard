import { invoke } from "./client";
import type { AppKind } from "./shared";

/** Read-only local session metadata. The backend never exposes source paths. */
export interface SessionMeta {
  app: AppKind;
  sessionId: string;
  title: string;
  summary: string;
  projectDir: string | null;
  createdAt: string | null;
  lastActiveAt: string | null;
  resumeCommand: string;
}

export interface SessionMessage {
  role: string;
  content: string;
  at: string | null;
}

export interface SessionIssue {
  app: AppKind;
  message: string;
}

export interface SessionScan {
  sessions: SessionMeta[];
  issues: SessionIssue[];
}

/** Result of starting a supported CLI's resume command in a new terminal. */
export interface SessionResume {
  command: string;
  usedProjectDir: boolean;
}

export function listSessions(): Promise<SessionScan> {
  return invoke<SessionScan>("list_sessions");
}

export function getSessionMessages(app: AppKind, sessionId: string): Promise<SessionMessage[]> {
  return invoke<SessionMessage[]>("get_session_messages", { app, sessionId });
}

export function resumeSession(app: AppKind, sessionId: string): Promise<SessionResume> {
  return invoke<SessionResume>("resume_session", { app, sessionId });
}

/** Permanently removes the local session record the backend resolves itself. */
export function deleteSession(app: AppKind, sessionId: string): Promise<void> {
  return invoke<void>("delete_session", { app, sessionId });
}
