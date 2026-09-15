import type { CreateServer } from "./mcp-json";
import type { McpMetadata } from "../../../api/client";

const PRESETS = [
  { value: "fetch", label: "fetch · 网页抓取", command: "uvx", args: ["mcp-server-fetch"] },
  { value: "time", label: "time · 时间与时区", command: "uvx", args: ["mcp-server-time"] },
  { value: "memory", label: "memory · 知识图谱", command: "npx", args: ["-y", "@modelcontextprotocol/server-memory"] },
  { value: "sequential-thinking", label: "sequential-thinking · 逐步推理", command: "npx", args: ["-y", "@modelcontextprotocol/server-sequential-thinking"] },
  { value: "context7", label: "context7 · 库文档", command: "npx", args: ["-y", "@upstash/context7-mcp"] },
];

export const MCP_PRESET_OPTIONS = [
  { value: "custom", label: "自定义" },
  ...PRESETS.map(({ value, label }) => ({ value, label })),
];

/** Presets stay in their portable form; the backend Claude render wraps
 * shell-shim launchers as `cmd /c …` on Windows and unwraps on import. */
export function presetServer(id: string): CreateServer {
  const preset = PRESETS.find(({ value }) => value === id);
  if (!preset) throw new Error("未知 MCP 预设");
  return { type: "stdio", command: preset.command, args: [...preset.args], env: {} };
}

export function presetMetadata(id: string): McpMetadata {
  const preset = PRESETS.find(({ value }) => value === id);
  if (!preset) throw new Error("未知 MCP 预设");
  const tags: Record<string, string[]> = {
    fetch: ["stdio", "http", "web"], time: ["stdio", "time", "utility"],
    memory: ["stdio", "memory", "graph"], "sequential-thinking": ["stdio", "thinking", "reasoning"],
    context7: ["stdio", "docs", "search"],
  };
  const section = id === "sequential-thinking" ? "sequentialthinking" : id;
  return {
    displayName: preset.args.at(-1), description: preset.label.split(" · ")[1], tags: [...tags[id]],
    homepage: id === "context7" ? "https://context7.com" : "https://github.com/modelcontextprotocol/servers",
    docs: id === "context7" ? "https://github.com/upstash/context7/blob/master/README.md"
      : "https://github.com/modelcontextprotocol/servers/tree/main/src/" + section,
  };
}
