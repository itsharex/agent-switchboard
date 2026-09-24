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
  alias: string | null;
  pinned: boolean;
  tags: string[];
}

export interface SessionMessage {
  id: string;
  role: string;
  content: string;
  at: string | null;
}

export interface SessionIssue {
  app: AppKind;
  message: string;
}

export interface SessionSearchHit {
  session: SessionMeta;
  messageId: string | null;
  excerpt: string;
}

export interface SessionSearchPage {
  results: SessionSearchHit[];
  total: number;
  issues: SessionIssue[];
  projects: SessionProject[];
  tags: string[];
}

export interface SessionProject {
  app: AppKind;
  projectDir: string | null;
  /** Unique sessions in this project, scoped only by the selected client. */
  count: number;
}

export interface SessionSearchRequest {
  query: string;
  app: AppKind | null;
  offset: number;
  project: Pick<SessionProject, "app" | "projectDir"> | null;
  tag: string | null;
  pinnedOnly: boolean;
  /** Inclusive RFC3339 lower bound for lastActiveAt. */
  activeAfter: string | null;
  /** Exclusive RFC3339 upper bound for lastActiveAt. */
  activeBefore: string | null;
}

export type SessionOrganizationChange =
  | { kind: "pin"; pinned: boolean }
  | { kind: "alias"; alias: string | null }
  | { kind: "setTags" | "addTags" | "removeTags"; tags: string[] };

/** Result of starting a supported CLI's resume command in a new terminal. */
export interface SessionResume {
  command: string;
  usedProjectDir: boolean;
}

export function searchSessions(request: SessionSearchRequest): Promise<SessionSearchPage> {
  return invoke("search_sessions", { request });
}

export function updateSessionOrganization(
  requests: SessionDeleteRequest[], change: SessionOrganizationChange,
): Promise<SessionMeta[]> {
  return invoke("update_session_organization", { requests, change });
}

export interface SessionBookmark {
  id: string;
  app: AppKind;
  sessionId: string;
  sessionTitle: string;
  projectDir: string | null;
  messageId: string;
  role: string;
  content: string;
  at: string | null;
  savedAt: string;
}

export function exportSessionMarkdown(app: AppKind, sessionId: string): Promise<string | null> {
  return invoke("export_session_markdown", { app, sessionId });
}

export function listSessionBookmarks(): Promise<SessionBookmark[]> {
  return invoke("list_session_bookmarks");
}

export function saveSessionBookmark(app: AppKind, sessionId: string, messageId: string): Promise<SessionBookmark> {
  return invoke("save_session_bookmark", { app, sessionId, messageId });
}

export function deleteSessionBookmark(id: string): Promise<void> {
  return invoke("delete_session_bookmark", { id });
}

export function getSessionMetadata(app: AppKind, sessionId: string): Promise<SessionMeta> {
  return invoke("get_session_metadata", { app, sessionId });
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

export interface SessionDeleteRequest {
  app: AppKind;
  sessionId: string;
}

/** One line of a batch deletion: removed, or the reason the record stayed. */
export interface SessionDeleteOutcome {
  app: AppKind;
  sessionId: string;
  deleted: boolean;
  error: string | null;
}

/** Deletes several records against one backend scan; each item reports its own outcome. */
export function deleteSessions(requests: SessionDeleteRequest[]): Promise<SessionDeleteOutcome[]> {
  return invoke<SessionDeleteOutcome[]>("delete_sessions", { requests });
}
