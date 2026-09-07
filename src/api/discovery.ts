import { invoke } from "./client";
import type { AppKind } from "./shared";
import type { RouteState } from "./switching";
import type { ProviderDraft } from "./providers";

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

export interface ImportProposal {
  app: AppKind;
  draft: ProviderDraft;
  basis: string;
}

export interface DiscoveryReport {
  codex: DiscoveredFile;
  claude: DiscoveredFile;
  importProposals: ImportProposal[];
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
  skippedExisting: string[];
  notImported: CcSwitchSkip[];
}

export function scanCcswitch(): Promise<CcSwitchScan> {
  return invoke<CcSwitchScan>("scan_ccswitch");
}

export function importCcswitchProfiles(keys: string[]): Promise<CcSwitchImportOutcome> {
  return invoke<CcSwitchImportOutcome>("import_ccswitch_profiles", { keys });
}

export function discoverLocal(): Promise<DiscoveryReport> {
  return invoke<DiscoveryReport>("discover_local");
}

/** The previous successful scan, cached by the backend; null before the first
 * scan ever completed. */
export function discoverCached(): Promise<DiscoveryReport | null> {
  return invoke<DiscoveryReport | null>("discover_cached");
}
