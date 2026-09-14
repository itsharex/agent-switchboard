import { expect, it } from "vitest";
import { MCP_PRESET_OPTIONS, presetServer } from "./presets";

it.each([false, true])("uses the actual Python time server on Windows=%s", (windows) => {
  expect(presetServer("time", windows)).toEqual({ type: "stdio", command: "uvx", args: ["mcp-server-time"], env: {} });
});

it("keeps npx flags and package names as distinct arguments on both platforms", () => {
  expect(presetServer("memory", false)).toMatchObject({ command: "npx", args: ["-y", "@modelcontextprotocol/server-memory"] });
  expect(presetServer("memory", true)).toMatchObject({ command: "cmd", args: ["/c", "npx", "-y", "@modelcontextprotocol/server-memory"] });
});

it("returns fresh editable arrays for every preset application", () => {
  for (const { value } of MCP_PRESET_OPTIONS.filter((preset) => preset.value !== "custom")) {
    const first = presetServer(value, false);
    const second = presetServer(value, false);
    if (first.type !== "stdio" || second.type !== "stdio") throw new Error("Unexpected preset transport");
    first.args.push("--changed");
    expect(second.args).not.toContain("--changed");
  }
});
