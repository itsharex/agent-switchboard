import type { McpEditViewEnvelope } from "../../../api/client";

export const stdioEnvelope: Extract<McpEditViewEnvelope, { transport: "stdio" }> = {
  id: "ext-mcp-1", revision: 3, name: "docs", transport: "stdio", command: "npx",
  args: ["-y", "package name", " leading ", "", "two\nlines"],
  env: [{ name: "LOG_LEVEL", value: { mode: "plain", value: "debug" } },
    { name: "DOCS_TOKEN", value: { mode: "secretConfigured" } }],
  codexOptions: { cwd: "/srv/docs", startupTimeoutSec: 0, toolTimeoutSec: 30, required: false },
};

export const httpEnvelope: Extract<McpEditViewEnvelope, { transport: "http" }> = {
  id: "ext-mcp-2", revision: 5, name: "api", transport: "http", url: "https://mcp.test/v1",
  headers: [{ name: "X-Api-Key", value: { mode: "secretConfigured" } }],
  bearer: { mode: "secretConfigured" },
};
