import { invoke } from "./client";
import type { ClientSettingsSnapshot, SettingsValues } from "./settings";
export interface CodexCommonView {
  policy: { version: 1; disabledProfileIds: string[] }; revision: string; settings: ClientSettingsSnapshot;
  fragment: { text: string; revision: string };
}
export const getCodexCommonConfig = (): Promise<CodexCommonView> => invoke("get_codex_common_config");
export const extractCodexCommonConfig = (): Promise<SettingsValues> => invoke("extract_codex_common_config");
export const setCodexCommonConfigEnabled = (profileId: string, enabled: boolean, expectedRevision: string): Promise<CodexCommonView> =>
  invoke("set_codex_common_config_enabled", { profileId, enabled, expectedRevision });
export const saveCodexCommonFragment = (text: string, expectedRevision: string): Promise<CodexCommonView> =>
  invoke("save_codex_common_fragment", { text, expectedRevision });
