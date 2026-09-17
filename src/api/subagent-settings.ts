import { invoke } from "./client";
import type { SettingValue } from "./settings";

/** The three Codex-global `[agents]` runtime intents. They are applied only
 * through the unified client-configuration transaction. */
export interface CodexSubagentSettings {
  enabled: SettingValue;
  maxConcurrentThreadsPerSession: SettingValue;
  interruptMessage: SettingValue;
}

/** Current values read from the real user-level Codex configuration. */
export interface CodexSubagentSettingsSnapshot {
  app: "codex";
  settings: CodexSubagentSettings;
  configHash: string;
  fileExists: boolean;
}

export function getCodexSubagentSettings(): Promise<CodexSubagentSettingsSnapshot> {
  return invoke<CodexSubagentSettingsSnapshot>("get_codex_subagent_settings");
}
