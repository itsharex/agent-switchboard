import { invoke } from "./client";
import type { SettingValue } from "./settings";

/** The three Codex-global `[agents]` runtime intents. This resource is
 * independent from application-owned client preferences: automatic removes
 * only its corresponding host key when the user confirms an apply. Provider
 * profiles own the default model and reasoning effort. */
export interface CodexSubagentSettings {
  enabled: SettingValue;
  maxConcurrentThreadsPerSession: SettingValue;
  interruptMessage: SettingValue;
}

/** Current values read from the real user-level Codex configuration. The
 * backend keeps the absolute path private and supplies the optimistic file
 * revision required by preview and apply. */
export interface CodexSubagentSettingsSnapshot {
  app: "codex";
  settings: CodexSubagentSettings;
  configHash: string;
  fileExists: boolean;
  deprecatedKeys: string[];
}

/** One redacted complete-file candidate. `renderedHash` identifies the exact
 * unredacted candidate accepted by the write transaction. */
export interface CodexSubagentSettingsPreview {
  app: "codex";
  target: string;
  content: string;
  configHash: string;
  renderedHash: string;
}

/** The confirmation-bound input accepted by the dedicated write command. */
export interface CodexSubagentSettingsPlan {
  settings: CodexSubagentSettings;
  expectedHash: string;
  expectedTargetExisted: boolean;
  renderedHash: string;
}

/** Reads only the Codex-global subagent runtime controls from the real
 * user-level configuration. No renderer-supplied path or provider profile is involved. */
export function getCodexSubagentSettings(): Promise<CodexSubagentSettingsSnapshot> {
  return invoke<CodexSubagentSettingsSnapshot>("get_codex_subagent_settings");
}

/** Renders a redacted whole-file candidate against the exact snapshot that
 * supplied `expectedHash`; it never writes the user configuration. */
export function previewCodexSubagentSettings(
  settings: CodexSubagentSettings,
  expectedHash: string,
): Promise<CodexSubagentSettingsPreview> {
  return invoke<CodexSubagentSettingsPreview>(
    "preview_codex_subagent_settings_command",
    { settings, expectedHash },
  );
}

/** Applies the one candidate the user explicitly previewed and confirmed. */
export function applyCodexSubagentSettings(
  plan: CodexSubagentSettingsPlan,
  confirmWrite: boolean,
): Promise<CodexSubagentSettingsSnapshot> {
  return invoke<CodexSubagentSettingsSnapshot>("apply_codex_subagent_settings", {
    plan,
    confirmWrite,
  });
}
