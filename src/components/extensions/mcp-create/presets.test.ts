import { expect, it } from "vitest";
import { MCP_PRESET_OPTIONS, presetServer } from "./presets";

it("uses the actual Python time server", () => {
  expect(presetServer("time")).toEqual({ type: "stdio", command: "uvx", args: ["mcp-server-time"], env: {} });
});

it("keeps npx presets portable; the backend Claude render owns the Windows cmd wrapper", () => {
  expect(presetServer("memory")).toMatchObject({ command: "npx", args: ["-y", "@modelcontextprotocol/server-memory"] });
});

it("returns fresh editable arrays for every preset application", () => {
  for (const { value } of MCP_PRESET_OPTIONS.filter((preset) => preset.value !== "custom")) {
    const first = presetServer(value);
    const second = presetServer(value);
    if (first.type !== "stdio" || second.type !== "stdio") throw new Error("Unexpected preset transport");
    first.args.push("--changed");
    expect(second.args).not.toContain("--changed");
  }
});
