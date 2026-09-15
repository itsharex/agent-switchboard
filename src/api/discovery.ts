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

export function scanCcswitch(): Promise<CcSwitchScan> {
  return invoke<CcSwitchScan>("scan_ccswitch");
}

/** One-click batch import for every selected row. Claude rows and the Codex
 * official record land in the generic store; third-party Codex rows are
 * completed into the strict store entirely inside the backend, so imported
 * credentials never cross the IPC boundary. */
export function importCcswitchClaudeProfiles(keys: string[]): Promise<CcSwitchImportOutcome> {
  return invoke<CcSwitchImportOutcome>("import_ccswitch_claude_profiles", { keys });
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
