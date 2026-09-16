import { invoke } from "./client";

export type CodexEnvSource =
  | { kind: "system"; location: string }
  | { kind: "file"; path: string; line: number };

export interface CodexEnvConflict {
  varName: string;
  /** Secret-shaped values arrive redacted; the backup keeps the real value. */
  valuePreview: string;
  source: CodexEnvSource;
}

export interface CodexEnvScan {
  conflicts: CodexEnvConflict[];
  revision: string;
}

export interface CodexEnvSelection {
  varName: string;
  source: CodexEnvSource;
}

export interface CodexEnvBackup {
  fileName: string;
  createdAt: string;
  entries: CodexEnvConflict[];
}

export const scanCodexEnvConflicts = (): Promise<CodexEnvScan> => invoke("scan_codex_env_conflicts");

export const removeCodexEnvConflicts = (
  selections: CodexEnvSelection[], expectedRevision: string, confirmWrite: boolean,
): Promise<CodexEnvBackup> => invoke("remove_codex_env_conflicts", { selections, expectedRevision, confirmWrite });

export const listCodexEnvBackups = (): Promise<CodexEnvBackup[]> => invoke("list_codex_env_backups");

export const restoreCodexEnvBackup = (fileName: string, confirmWrite: boolean): Promise<number> =>
  invoke("restore_codex_env_backup", { fileName, confirmWrite });

export function describeCodexEnvSource(source: CodexEnvSource): string {
  return source.kind === "system" ? source.location : `${source.path}:${source.line}`;
}
