import { invoke } from "./client";
import type { AppKind, UpstreamProtocol } from "./shared";
import type { RouteState, ConfigWriteRecord } from "./switching";

/** Closed, renderer-safe runtime events emitted by the application backend. */
export type RuntimeLogAction =
  | "appStarted"
  | "appSettingsSaved"
  | "appSettingsRepaired"
  | "profileStoreReset"
  | "profileCreated"
  | "profileUpdated"
  | "profileDeleted"
  | "profilesReordered"
  | "profileImported"
  | "globalPromptDocumentSaved"
  | "configurationSwitched"
  | "backupRestored"
  | "switchUndone"
  | "staleLockRecovered"
  | "cloudBackupSettingsSaved"
  | "cloudBackupConnectionTested"
  | "cloudBackupUploaded"
  | "cloudBackupRestored"
  | "sessionResumed"
  | "ccSwitchProfilesImported"
  | "officialLoginCompleted";

/** Persisted recording threshold. `silent` stops future event writes. */
export type RuntimeLogLevel = "debug" | "info" | "warn" | "error" | "silent";

/** Severity of one event already written to the application log. */
export type RuntimeLogSeverity = Exclude<RuntimeLogLevel, "silent">;

/** One non-secret application runtime event from the app-owned log files. */
export interface RuntimeLogEntry {
  at: string;
  level: RuntimeLogSeverity;
  action: RuntimeLogAction;
  errorCode?: string;
}

export type MatchStatus =
  | { kind: "matchesProfile"; profileId: string; profileName: string }
  | { kind: "profileChanged"; profileName: string }
  | { kind: "restoredBackup"; at: string }
  | { kind: "externallyModified"; at: string }
  | { kind: "unmanaged" }
  | { kind: "unknown" };

export interface ConfigFileStatus {
  app: AppKind;
  path: string;
  exists: boolean;
  syntaxOk: boolean;
  route: RouteState | null;
  readError: string | null;
  activeProfileId: string | null;
  matchStatus: MatchStatus;
  lastSwitch: ConfigWriteRecord | null;
}

/** Read-only application metadata for the overview footer. It never includes
 * client configuration, credentials, or account information. */
export interface RuntimeOverview {
  appVersion: string;
  buildMode: "debug" | "release";
  platform: string;
  architecture: string;
  transport: RuntimeTransport;
  appDataPath: string;
}

export type RuntimeTransport =
  | { kind: "desktopProtocol" }
  | { kind: "webDevelopment"; host: string; port: number; healthStatus: number };

/** One active loopback route on the gateway status page. The page resolves
 * the display name from its own profile snapshot via profileId. */
export interface GatewayRouteStatus {
  app: AppKind;
  profileId: string;
  upstreamProtocol: UpstreamProtocol;
}

/** One completed gateway request. `status` is the HTTP status, or null when
 * a WebSocket exchange ended without a complete converted answer.
 * `profileId` and `upstreamProtocol` are set only when the request matched an
 * active route; rejected traffic stays unattributed. */
export interface GatewaySample {
  atMs: number;
  app: AppKind;
  profileId: string | null;
  clientProtocol: UpstreamProtocol;
  upstreamProtocol: UpstreamProtocol | null;
  status: number | null;
  durationMs: number;
  requestBytes: number;
  responseBytes: number;
}

export interface GatewayMetricsStatus {
  startedAtMs: number;
  totalRequests: number;
  failedRequests: number;
  samples: GatewaySample[];
}

export interface GatewayStatus {
  port: number;
  baseUrl: string;
  routes: GatewayRouteStatus[];
  metrics: GatewayMetricsStatus;
}

export function getConfigStatus(): Promise<ConfigFileStatus[]> {
  return invoke<ConfigFileStatus[]>("config_status");
}

export function getRuntimeOverview(): Promise<RuntimeOverview> {
  return invoke<RuntimeOverview>("runtime_overview");
}

export function getGatewayStatus(): Promise<GatewayStatus> {
  return invoke<GatewayStatus>("gateway_status");
}

export function listRuntimeLogs(): Promise<RuntimeLogEntry[]> {
  return invoke<RuntimeLogEntry[]>("list_runtime_logs");
}

/** Opens the app-owned runtime-log directory without exposing its path to the UI. */
export function openRuntimeLogDir(): Promise<void> {
  return invoke<void>("open_runtime_log_dir");
}
