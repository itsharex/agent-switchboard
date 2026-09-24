import { uiMessage } from "../../../i18n/errors";
import type { CreateServer } from "./mcp-json";
import type { McpMetadata } from "../../../api/client";
import { tr } from "../../../i18n/current";
import type { MessageKey } from "../../../i18n/messages";

interface Preset {
  value: string;
  labelKey: MessageKey;
  command: string;
  args: string[];
}

const PRESETS: Preset[] = [
  { value: "fetch", labelKey: "mcp.preset.fetch", command: "uvx", args: ["mcp-server-fetch"] },
  { value: "time", labelKey: "mcp.preset.time", command: "uvx", args: ["mcp-server-time"] },
  { value: "memory", labelKey: "mcp.preset.memory", command: "npx", args: ["-y", "@modelcontextprotocol/server-memory"] },
  { value: "sequential-thinking", labelKey: "mcp.preset.sequential-thinking", command: "npx", args: ["-y", "@modelcontextprotocol/server-sequential-thinking"] },
  { value: "context7", labelKey: "mcp.preset.context7", command: "npx", args: ["-y", "@upstash/context7-mcp"] },
];

/** Built at call time so the labels follow the active language instead of
 * baking display text in at module load. */
export function mcpPresetOptions() {
  return [
    { value: "custom", label: tr("mcp.preset.custom") },
    ...PRESETS.map(({ value, labelKey }) => ({ value, label: tr(labelKey) })),
  ];
}

/** Presets stay in their portable form; the backend Claude render wraps
 * shell-shim launchers as `cmd /c …` on Windows and unwraps on import. */
export function presetServer(id: string): CreateServer {
  const preset = PRESETS.find(({ value }) => value === id);
  if (!preset) throw uiMessage("mcp.error.presetUnknown");
  return { type: "stdio", command: preset.command, args: [...preset.args], env: {} };
}

export function presetMetadata(id: string): McpMetadata {
  const preset = PRESETS.find(({ value }) => value === id);
  if (!preset) throw uiMessage("mcp.error.presetUnknown");
  const tags: Record<string, string[]> = {
    fetch: ["stdio", "http", "web"], time: ["stdio", "time", "utility"],
    memory: ["stdio", "memory", "graph"], "sequential-thinking": ["stdio", "thinking", "reasoning"],
    context7: ["stdio", "docs", "search"],
  };
  const section = id === "sequential-thinking" ? "sequentialthinking" : id;
  return {
    displayName: preset.args.at(-1), description: tr(preset.labelKey).split(" · ")[1], tags: [...tags[id]],
    homepage: id === "context7" ? "https://context7.com" : "https://github.com/modelcontextprotocol/servers",
    docs: id === "context7" ? "https://github.com/upstash/context7/blob/master/README.md"
      : "https://github.com/modelcontextprotocol/servers/tree/main/src/" + section,
  };
}
