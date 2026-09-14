import { expect, it } from "vitest";
import { materializeMcpDraft, validateMcpSource } from "./mcp-draft";
import { formatMcpJson, parseMcpJson } from "./mcp-json";
import { initializeWizard, wizardSource } from "./wizard-state";

it("round-trips all modeled Codex options through JSON, wizard and creation", async () => {
  const codexOptions = { cwd: "C:/Program Files/docs", startupTimeoutSec: 0, toolTimeoutSec: 60, required: false };
  const source = parseMcpJson(JSON.stringify({ command: "run", codexOptions }), "docs");
  validateMcpSource(source, ["codex"]);
  expect(parseMcpJson(formatMcpJson(source.server), source.name)).toEqual(source);
  expect(wizardSource(source.name, initializeWizard(source.server))).toEqual(source);
  expect((await materializeMcpDraft(source, async () => null)).payload).toMatchObject({ codexOptions });
});

it.each([
  [false, "JSON 对象"], [{ cwd: " " }, "工作目录"], [{ cwd: "a\0b" }, "工作目录"],
  [{ startupTimeoutSec: -1 }, "启动超时"], [{ startupTimeoutSec: 1.5 }, "启动超时"],
  [{ toolTimeoutSec: "30" }, "工具超时"], [{ toolTimeoutSec: Number.MAX_SAFE_INTEGER + 1 }, "工具超时"],
  [{ required: "false" }, "布尔值"], [{ unknown: true }, "不支持的字段"],
])("refuses malformed Codex options before entering the wizard: %j", (codexOptions, message) => {
  expect(() => parseMcpJson(JSON.stringify({ command: "run", codexOptions }), "docs")).toThrow(message as string);
});

it("does not accept Codex-only stdio options on remote transports", () => {
  expect(() => parseMcpJson('{"url":"https://test.invalid","codexOptions":{"cwd":"/srv"}}', "docs"))
    .toThrow("不支持的字段");
});

it("accepts native Codex option names and normalizes them to the same typed owner", async () => {
  const source = parseMcpJson(JSON.stringify({ command: "run", cwd: "/srv/docs", startup_timeout_sec: 0,
    tool_timeout_sec: 30, required: false }), "docs");
  const draft = await materializeMcpDraft(source, async () => null);
  expect(draft.payload).toMatchObject({ codexOptions: { cwd: "/srv/docs", startupTimeoutSec: 0, toolTimeoutSec: 30, required: false } });
  expect(parseMcpJson(formatMcpJson(source.server), source.name)).toEqual(source);
});

it("refuses conflicting native and nested Codex options", () => {
  expect(() => parseMcpJson('{"command":"run","required":false,"codexOptions":{"required":true}}', "docs"))
    .toThrow("不能同时使用");
});
