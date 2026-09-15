import { invoke } from "./client";

export type ClaudeEnvSource =
  | { kind: "system"; location: string }
  | { kind: "file"; path: string; line: number };

export interface ClaudeEnvConflict {
  varName: string;
  /** Secret-shaped values arrive redacted; the backup keeps the real value. */
  valuePreview: string;
  source: ClaudeEnvSource;
}

export interface ClaudeEnvScan {
  conflicts: ClaudeEnvConflict[];
  revision: string;
}

export interface ClaudeEnvSelection {
  varName: string;
  source: ClaudeEnvSource;
}

export interface ClaudeEnvBackup {
  fileName: string;
  createdAt: string;
  entries: ClaudeEnvConflict[];
}

export const scanClaudeEnvConflicts = (): Promise<ClaudeEnvScan> => invoke("scan_claude_env_conflicts");

export const removeClaudeEnvConflicts = (
  selections: ClaudeEnvSelection[], expectedRevision: string, confirmWrite: boolean,
): Promise<ClaudeEnvBackup> => invoke("remove_claude_env_conflicts", { selections, expectedRevision, confirmWrite });

export const listClaudeEnvBackups = (): Promise<ClaudeEnvBackup[]> => invoke("list_claude_env_backups");

export const restoreClaudeEnvBackup = (fileName: string, confirmWrite: boolean): Promise<number> =>
  invoke("restore_claude_env_backup", { fileName, confirmWrite });

export function describeClaudeEnvSource(source: ClaudeEnvSource): string {
  return source.kind === "system" ? source.location : `${source.path}:${source.line}`;
}
