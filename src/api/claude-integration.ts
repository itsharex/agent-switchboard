import { invoke } from "./client";
import type { KeyChange } from "./switching";

export type ClaudeIntegrationFlag = "plugin" | "onboarding";

export interface ClaudeIntegrationPolicy {
  /** After a Claude switch, keep the plugin marker in step with the route. */
  pluginIntegration: boolean;
}

export interface ClaudeIntegrationFlagState {
  flag: ClaudeIntegrationFlag;
  target: string;
  exists: boolean;
  applied: boolean;
  contentHash: string;
}

export interface ClaudeIntegrationView {
  policy: ClaudeIntegrationPolicy;
  flags: ClaudeIntegrationFlagState[];
}

export interface ClaudeIntegrationPreview {
  flag: ClaudeIntegrationFlag;
  enable: boolean;
  target: string;
  contentHash: string;
  targetExisted: boolean;
  renderedHash: string;
  changes: KeyChange[];
}

export const getClaudeIntegration = (): Promise<ClaudeIntegrationView> =>
  invoke("get_claude_integration");

export const setClaudeIntegrationPolicy = (
  policy: ClaudeIntegrationPolicy, confirmWrite: boolean,
): Promise<ClaudeIntegrationView> => invoke("set_claude_integration_policy", { policy, confirmWrite });

export const previewClaudeIntegration = (
  flag: ClaudeIntegrationFlag, enable: boolean,
): Promise<ClaudeIntegrationPreview> => invoke("preview_claude_integration", { flag, enable });

export const applyClaudeIntegration = (
  preview: ClaudeIntegrationPreview, confirmWrite: boolean,
): Promise<ClaudeIntegrationView> => invoke("apply_claude_integration", { preview, confirmWrite });
