import { invoke } from "./client";
import type { ProviderEndpoint } from "./providers";

/** Custom targets of one Codex provider; the primary endpoint is read-only here. */
export interface CodexEndpointsView {
  providerId: string;
  fileHash: string;
  primary: string;
  endpoints: ProviderEndpoint[];
}

/** One reachability row: any HTTP answer is reachable; no credential is sent. */
export interface CodexEndpointLatency {
  url: string;
  latencyMs: number | null;
  status: number | null;
  error: string | null;
}

export const listCodexEndpoints = (providerId: string): Promise<CodexEndpointsView> =>
  invoke("list_codex_endpoints", { providerId });

export const addCodexEndpoint = (
  providerId: string, url: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<CodexEndpointsView> => invoke("add_codex_endpoint", { providerId, url, expectedFileHash, confirmWrite });

export const removeCodexEndpoint = (
  providerId: string, url: string, expectedFileHash: string, confirmWrite: boolean,
): Promise<CodexEndpointsView> => invoke("remove_codex_endpoint", { providerId, url, expectedFileHash, confirmWrite });

export const testCodexEndpoints = (urls: string[]): Promise<CodexEndpointLatency[]> =>
  invoke("test_codex_endpoints", { urls });
