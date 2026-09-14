import { invoke } from "./client";
import type { AppKind } from "./shared";
import type { RuntimeLogLevel } from "./status";

/** One backend-resolved global instruction document. Its absolute path never
 * crosses the renderer boundary; the hash protects against stale saves. */
export interface GlobalPromptDocument {
  app: AppKind;
  fileName: string;
  content: string;
  contentHash: string;
  exists: boolean;
}

/** One concrete client configuration value. */
export type ConfigValue = boolean | string | number;

/** One application-owned setting intent. Automatic means that no line/key is
 * written to the client configuration; explicit values are always projected. */
export type SettingValue =
  | { mode: "automatic" }
  | { mode: "explicit"; value: ConfigValue };

/** Complete values for one backend-owned settings catalog. */
export interface SettingsValues {
  /** Claude-only supplemental fields; visual preferences keep their existing owner. */
  claudeExtra?: Record<string, unknown>;
  settings: Record<string, SettingValue>;
}

export interface SettingChoiceOption {
  value: string;
  label: string;
}

/** One backend-owned parameter or client preference. */
export type SettingSpec =
  | {
      key: string;
      label: string;
      group: string;
      control: "toggle";
      options: [];
    }
  | {
      key: string;
      label: string;
      group: string;
      control: "slider" | "segment";
      options: SettingChoiceOption[];
    }
  | {
      key: string;
      label: string;
      group: string;
      control: "model";
      options: [];
    };

/** One official configuration family with its real editing boundary. Paths
 * are labels from the typed backend directory, never renderer-supplied file
 * targets. */
export interface OfficialSettingDirectoryEntry {
  title: string;
  paths: string[];
  disposition: "direct" | "separateModule" | "preserveOnly";
  detail: string;
}

/** Full typed client-preference editing model. `settingsHash` is the
 * optimistic application-store revision; it is unrelated to client-file
 * hashes. */
export interface ClientSettingsEditor {
  app: AppKind;
  settings: SettingsValues;
  settingsHash: string;
  groups: string[];
  specs: SettingSpec[];
  directory: OfficialSettingDirectoryEntry[];
}

export interface ClientSettingsSnapshot {
  settings: SettingsValues;
  settingsHash: string;
}

export interface ProviderParametersCatalog {
  app: AppKind;
  defaults: SettingsValues;
  groups: string[];
  specs: SettingSpec[];
}

export function getProviderParametersCatalog(app: AppKind): Promise<ProviderParametersCatalog> {
  return invoke<ProviderParametersCatalog>("get_provider_parameters_catalog", { target: app });
}

/** An editable rendering of the current draft's shared settings only. */
export interface ClientSettingsPreview {
  app: AppKind;
  target: string;
  content: string;
}

/** Reads the stored client-preference values plus the catalog that can edit
 * them. This does not read a real Codex or Claude Code configuration file. */
export function getClientSettingsEditor(app: AppKind): Promise<ClientSettingsEditor> {
  return invoke<ClientSettingsEditor>("get_client_settings_editor", { target: app });
}

/** Saves desired application state only. A supplier must subsequently be
 * re-applied through the normal switch flow to project it into a client file. */
export function saveClientSettings(
  app: AppKind,
  settings: SettingsValues,
  expectedSettingsHash: string,
): Promise<ClientSettingsSnapshot> {
  return invoke<ClientSettingsSnapshot>("save_client_settings", {
    target: app,
    settings,
    expectedSettingsHash,
  });
}

/** Renders the current client-preference draft without reading or writing a
 * real client file. */
export function previewClientSettings(
  app: AppKind,
  settings: SettingsValues,
): Promise<ClientSettingsPreview> {
  return invoke<ClientSettingsPreview>("preview_client_settings", {
    target: app,
    settings,
  });
}

/** Parses a manually edited client fragment without reading or writing a real
 * client file. Missing keys become automatic settings. */
export function parseClientSettings(
  app: AppKind,
  content: string,
): Promise<SettingsValues> {
  return invoke<SettingsValues>("parse_client_settings", {
    target: app,
    content,
  });
}

export function getGlobalPromptDocument(app: AppKind): Promise<GlobalPromptDocument> {
  return invoke<GlobalPromptDocument>("get_global_prompt_document", { target: app });
}

export function saveGlobalPromptDocument(
  app: AppKind,
  content: string,
  expectedHash: string,
  confirmWrite: boolean,
): Promise<GlobalPromptDocument> {
  return invoke<GlobalPromptDocument>("save_global_prompt_document", {
    target: app,
    content,
    expectedHash,
    confirmWrite,
  });
}

export type CloseBehavior = "hideToTray" | "exit";
export type ThemePreference = "system" | "light" | "dark";
export type MotionPreference = "system" | "reduce";

/** Application-runtime desktop preferences; separate from client config. */
export interface AppSettings {
  closeBehavior: CloseBehavior;
  theme: ThemePreference;
  motion: MotionPreference;
  alwaysOnTop: boolean;
  launchAtLogin: boolean;
  hardwareAcceleration: boolean;
  /** Font family for display and interface text; the value is quoted
   * verbatim as a CSS font-family, so it must be a plain family name. */
  interfaceFont: string;
  /** Threshold used for future application runtime-event recording. */
  runtimeLogLevel: RuntimeLogLevel;
  /** Provider ids whose usage panel is collapsed; any other provider's
   * panel is expanded. */
  collapsedUsageIds: string[];
}

/** Public connection coordinates for a user-owned Supabase project. The
 * Supabase account password and cloud-backup password are action-only inputs
 * and are never persisted. */
export interface CloudBackupSettings {
  projectUrl: string;
  publishableKey: string;
  email: string;
}

export interface CloudBackupResult {
  updatedAt: string;
  profileCount: number;
  /** The restored encrypted backup was upgraded to the current snapshot schema. */
  migrated: boolean;
}

export function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_app_settings");
}

export function setAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("set_app_settings", { settings });
}

/** Replaces an invalid settings file with defaults; a readable file refuses. */
export function repairAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("repair_app_settings");
}

export function getCloudBackupSettings(): Promise<CloudBackupSettings | null> {
  return invoke<CloudBackupSettings | null>("get_cloud_backup_settings");
}

export function setCloudBackupSettings(
  settings: CloudBackupSettings,
): Promise<CloudBackupSettings> {
  return invoke<CloudBackupSettings>("set_cloud_backup_settings", { settings });
}

export function getCloudBackupSetupSql(): Promise<string> {
  return invoke<string>("cloud_backup_setup_sql");
}

/** Verifies an unsaved cloud-backup draft without writing remote data. */
export function testCloudBackupConnection(
  settings: CloudBackupSettings,
  accountPassword: string,
): Promise<void> {
  return invoke<void>("test_cloud_backup_connection", { settings, accountPassword });
}

export function uploadCloudBackup(
  accountPassword: string,
  backupPassword: string,
  confirmWrite: boolean,
): Promise<CloudBackupResult> {
  return invoke<CloudBackupResult>("upload_cloud_backup", {
    accountPassword,
    backupPassword,
    confirmWrite,
  });
}

export function restoreCloudBackup(
  accountPassword: string,
  backupPassword: string,
  confirmWrite: boolean,
): Promise<CloudBackupResult> {
  return invoke<CloudBackupResult>("restore_cloud_backup", {
    accountPassword,
    backupPassword,
    confirmWrite,
  });
}

/** Installed system font families, offered by the interface-font picker. */
export function listSystemFonts(): Promise<string[]> {
  return invoke<string[]>("list_system_fonts");
}

/** Dev-machine debug affordance: toggles the WebView inspector. */
export function toggleDevtools(): Promise<void> {
  return invoke<void>("toggle_devtools");
}
