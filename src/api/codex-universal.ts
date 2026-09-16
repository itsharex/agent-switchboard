import { invoke } from "./client";
import type { CodexUpstream } from "./providers";

/** One shared connection with its generated Codex link, if any. */
export interface CodexUniversalView {
  id: string;
  name: string;
  endpoint: string;
  upstream: CodexUpstream;
  hasApiKey: boolean;
  defaultModel: string;
  modelCount: number;
  generatedCodexProfileId: string | null;
  linkedProfileName: string | null;
}

export interface CodexUniversalList {
  revision: string;
  providers: CodexUniversalView[];
}

export interface CodexUniversalSyncOutcome {
  created: boolean;
  unchanged: boolean;
  warnings: string[];
  profileId: string;
  list: CodexUniversalList;
}

export interface CodexUniversalInput {
  name: string;
  endpoint: string;
  upstream: CodexUpstream;
  apiKey: string;
  models: string[];
}

export const listCodexUniversalProviders = (): Promise<CodexUniversalList> =>
  invoke("list_codex_universal_providers", {});

export const createCodexUniversalProvider = (input: CodexUniversalInput): Promise<CodexUniversalList> =>
  invoke("create_codex_universal_provider", { ...input });

export const updateCodexUniversalProvider = (
  id: string, expectedRevision: string, input: CodexUniversalInput,
): Promise<CodexUniversalList> =>
  invoke("update_codex_universal_provider", { id, expectedRevision, ...input });

export const deleteCodexUniversalProvider = (
  id: string, expectedRevision: string,
): Promise<CodexUniversalList> =>
  invoke("delete_codex_universal_provider", { id, expectedRevision });

/** Generates the Codex profile on first use, then syncs shared facts only. */
export const syncCodexUniversalProfile = (
  id: string, expectedRevision: string,
): Promise<CodexUniversalSyncOutcome> =>
  invoke("sync_codex_universal_profile", { id, expectedRevision });
