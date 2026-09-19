import { invoke } from "./client";
import type { AppKind } from "./shared";
import type { RouteState } from "./switching";
import type { ProviderDraft } from "./providers";
import type { CodexUpstream } from "./providers";

export type DiscoveredState =
  | { kind: "missing" }
  | { kind: "readError"; message: string }
  | { kind: "parseError"; message: string; line: number | null }
  | { kind: "ok"; route: RouteState; managed: boolean; warnings: string[]; importable: boolean };

export interface DiscoveredFile {
  app: AppKind;
  path: string;
  exists: boolean;
  state: DiscoveredState;
}

export interface ClaudeImportProposal {
  draft: ProviderDraft;
  basis: string;
}

export interface CodexImportProposal {
  name: string;
  providerName: string;
  model: string | null;
  upstream: CodexUpstream | null;
  catalogModelCount: number;
  apiKeyAvailable: boolean;
  official: boolean;
  basis: string;
  warnings: string[];
}

export interface CodexImportResult {
  id: string;
  name: string;
  official: boolean;
}

export interface DiscoveryReport {
  codex: DiscoveredFile;
  claude: DiscoveredFile;
  codexImportProposals: CodexImportProposal[];
  claudeImportProposals: ClaudeImportProposal[];
}

export interface CcSwitchSkip {
  key: string;
  appType: string;
  name: string;
  reason: string;
}

export interface CcSwitchScanItem {
  key: string;
  app: AppKind;
  routeMode: "official" | "custom";
  name: string;
  model: string | null;
  baseUrl: string | null;
  usageScriptImportable: boolean;
  usageScriptUpdatesExisting: boolean;
  endpointCandidates: number;
  warnings: string[];
  existing: boolean;
}

export interface CcSwitchScan {
  dbPath: string;
  providers: CcSwitchScanItem[];
  skipped: CcSwitchSkip[];
}

export interface CcSwitchImportOutcome {
  importedCount: number;
  usageScriptImportedCount: number;
  endpointCandidatesImported: number;
  skippedExisting: string[];
  notImported: CcSwitchSkip[];
}

/** `dbDirectory` is a user-picked folder that directly contains
 * `cc-switch.db`; null keeps the default home location. */
export function scanCcswitch(dbDirectory: string | null): Promise<CcSwitchScan> {
  return invoke<CcSwitchScan>("scan_ccswitch", { dbDirectory });
}

export interface ProviderSqlSkip {
  name: string;
  reason: string;
}

export interface ProviderSqlExport {
  exportedCount: number;
  skipped: ProviderSqlSkip[];
}

/** Writes every stored provider document — complete configuration included —
 * to the chosen SQL file. The companion 「导入 SQL」 panel applies the file on
 * another device without any command line. */
export function exportProvidersSql(targetPath: string): Promise<ProviderSqlExport> {
  return invoke<ProviderSqlExport>("export_providers_sql", { targetPath });
}

export interface ProviderSqlScanItem {
  key: string;
  kind: "claude" | "codex_official" | "codex_custom";
  name: string;
  model: string | null;
  baseUrl: string | null;
  warnings: string[];
  /** A record with the same id already exists; importing overwrites it. */
  existing: boolean;
}

export interface ProviderSqlScan {
  sqlPath: string;
  providers: ProviderSqlScanItem[];
  skipped: ProviderSqlSkip[];
}

/** Applies one exported SQL file to the app-owned scratch database and
 * returns the typed preview. `sqlPath` must be re-passed to the import so
 * the backend re-applies the exact file the user previewed. */
export function applyProvidersSql(sqlPath: string): Promise<ProviderSqlScan> {
  return invoke<ProviderSqlScan>("apply_providers_sql", { sqlPath });
}

export interface ProviderSqlImportOutcome {
  importedCount: number;
  updatedCount: number;
  notImported: ProviderSqlSkip[];
}

export function importProvidersSql(
  ids: string[],
  sqlPath: string,
): Promise<ProviderSqlImportOutcome> {
  return invoke<ProviderSqlImportOutcome>("import_providers_sql", { ids, sqlPath });
}

/** One-click batch import for every selected row. Claude rows and the Codex
 * official record land in the generic store; third-party Codex rows are
 * completed into the strict store entirely inside the backend, so imported
 * credentials never cross the IPC boundary. `dbDirectory` must be the same
 * folder the scan used, so the freshness re-scan reads the previewed
 * database. */
export function importCcswitchClaudeProfiles(
  keys: string[],
  dbDirectory: string | null,
): Promise<CcSwitchImportOutcome> {
  return invoke<CcSwitchImportOutcome>("import_ccswitch_claude_profiles", { keys, dbDirectory });
}

export function discoverLocal(): Promise<DiscoveryReport> {
  return invoke<DiscoveryReport>("discover_local");
}

/** The previous successful scan, cached by the backend; null before the first
 * scan ever completed. */
export function discoverCached(): Promise<DiscoveryReport | null> {
  return invoke<DiscoveryReport | null>("discover_cached");
}

export function importDiscoveredCodexProfile(): Promise<CodexImportResult> {
  return invoke<CodexImportResult>("import_discovered_codex_profile");
}
