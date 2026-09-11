import { invoke } from "./client";
import type { AppKind } from "./shared";
import type { RouteState } from "./switching";
import type {
  CodexReasoningLevel,
  CodexRequestMode,
  CodexUpstream,
  ProviderDraft,
} from "./providers";
import type { SettingsValues } from "./settings";
import type { UsageQuery } from "./usage";

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

export interface DiscoveryReport {
  codex: DiscoveredFile;
  claude: DiscoveredFile;
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

/** Source-owned Codex facts for one imported row. The catalog limits and
 * capability statements the strict profile contract requires are completed
 * and confirmed in the editor; a seed never persists directly. */
export interface CodexCcSwitchCatalogSeed {
  model: string;
  contextWindow: number | null;
  images: boolean | null;
  defaultReasoningLevel: CodexReasoningLevel | null;
  reasoningLevels: CodexReasoningLevel[] | null;
}

export interface CodexCcSwitchSeed {
  name: string;
  endpoint: string;
  /** Empty when the source row exposed no usable credential. */
  apiKey: string;
  upstream: CodexUpstream;
  requestMode: CodexRequestMode;
  defaultModel: string;
  catalog: CodexCcSwitchCatalogSeed[];
  parameters: SettingsValues;
  notes: string | null;
  websiteUrl: string | null;
  usageQuery: UsageQuery | null;
  warnings: string[];
}

export function scanCcswitch(): Promise<CcSwitchScan> {
  return invoke<CcSwitchScan>("scan_ccswitch");
}

/** Batch import for Claude rows only. Codex rows are completed per row in
 * the editor via `prepareCcswitchCodexSeed`. */
export function importCcswitchClaudeProfiles(keys: string[]): Promise<CcSwitchImportOutcome> {
  return invoke<CcSwitchImportOutcome>("import_ccswitch_claude_profiles", { keys });
}

/** The completion seed for one deliberately chosen Codex row. The only
 * boundary where an imported credential reaches the renderer; nothing is
 * persisted by this command. */
export function prepareCcswitchCodexSeed(key: string): Promise<CodexCcSwitchSeed> {
  return invoke<CodexCcSwitchSeed>("prepare_ccswitch_codex_seed", { key });
}

export function discoverLocal(): Promise<DiscoveryReport> {
  return invoke<DiscoveryReport>("discover_local");
}

/** The previous successful scan, cached by the backend; null before the first
 * scan ever completed. */
export function discoverCached(): Promise<DiscoveryReport | null> {
  return invoke<DiscoveryReport | null>("discover_cached");
}
