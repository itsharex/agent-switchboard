import { invoke } from "./client";

/** One the source application `mcp_servers` row as seen by the read-only scan. */
export interface ClaudeSourceMcpServer {
  sourceId: string;
  name: string;
  transport: string;
  enabledForClaude: boolean;
  description: string | null;
  problem: string | null;
  existing: boolean;
}

export interface ClaudeMcpSource {
  sourceRevision: string;
  servers: ClaudeSourceMcpServer[];
}

export interface ClaudeMcpImportResult {
  imported: string[];
  unchanged: number;
  warnings: string[];
}

export const scanClaudeMcpSource = (sourcePath: string): Promise<ClaudeMcpSource> =>
  invoke("scan_claude_mcp_source", { sourcePath });

export const importClaudeMcpSource = (
  sourcePath: string, sourceIds: string[], sourceRevision: string, confirmWrite: boolean,
): Promise<ClaudeMcpImportResult> => invoke("import_claude_mcp_source", { sourcePath, sourceIds, sourceRevision, confirmWrite });
