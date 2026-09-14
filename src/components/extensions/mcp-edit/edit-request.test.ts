import { expect, it, vi } from "vitest";
import type { McpEditViewEnvelope } from "../../../api/client";
import { formatMcpJson, parseMcpJson } from "../mcp-create/mcp-json";
import { metadataDraft } from "../mcp-create/metadata";
import { buildMcpEdit } from "./edit-request";
import { mcpEditSource } from "./edit-source";
import { httpEnvelope, stdioEnvelope } from "./test-fixtures";

it.each([stdioEnvelope, httpEnvelope,
  { ...httpEnvelope, transport: "claudeSse" }, { ...httpEnvelope, transport: "claudeWs", url: "wss://mcp.test" },
] as McpEditViewEnvelope[])("round-trips $transport without replacing any field or revealing a secret", async (envelope) => {
  const original = mcpEditSource(envelope);
  const parsed = parseMcpJson(formatMcpJson(original.server), original.name, original.server);
  const put = vi.fn();
  expect(await buildMcpEdit(envelope, parsed, metadataDraft(), [], put)).toEqual({ expectedRevision: envelope.revision, fields: {} });
  expect(put).not.toHaveBeenCalled();
});

it("rejects stored markers at new slots, in plain positions and across transport changes", () => {
  const original = mcpEditSource(httpEnvelope).server;
  const kept = { mode: "secretConfigured" };
  for (const spec of [
    { url: "https://mcp.test", headers: { Copied: kept } },
    { type: "sse", url: "https://mcp.test", headers: { "X-Api-Key": kept } },
    { type: "stdio", command: "run", env: { "X-Api-Key": kept } },
    { url: "https://mcp.test", headers: { "X-Api-Key": { ...kept, reference: "forged" } } },
  ]) expect(() => parseMcpJson(JSON.stringify(spec), "test", original)).toThrow();
  expect(() => parseMcpJson(JSON.stringify({ command: "run", env: { LOG_LEVEL: kept } }), "docs", mcpEditSource(stdioEnvelope).server)).toThrow();
});

it.each(["stdio", "http", "sse", "ws"])("builds complete %s replacements with no fields from the old transport", async (type) => {
  const envelope = type === "stdio" ? httpEnvelope : stdioEnvelope;
  const spec = type === "stdio" ? { type, command: "run", codexOptions: { startupTimeoutSec: 0, required: false } }
    : { type, url: type === "ws" ? "wss://mcp.test" : "https://mcp.test", headers: { "X-Key": "sk-new-token" } };
  const put = vi.fn(async () => "new-reference");
  const edit = await buildMcpEdit(envelope, parseMcpJson(JSON.stringify(spec), "renamed"), metadataDraft(), [], put);
  expect(edit.serverKey).toBe("renamed");
  expect(edit.fields).toEqual({});
  expect(edit.transport?.transport).toBe(type === "sse" ? "claudeSse" : type === "ws" ? "claudeWs" : type);
  expect(JSON.stringify(edit)).not.toContain("secretConfigured");
  expect(JSON.stringify(edit)).not.toContain("sk-new-token");
});

it("keeps metadata on unchanged documents and clears it with an explicit delete", async () => {
  const envelope = { ...stdioEnvelope, mcpMetadata: { displayName: "Docs", tags: ["search"] } };
  const original = mcpEditSource(envelope);
  expect(await buildMcpEdit(envelope, original, metadataDraft(envelope.mcpMetadata), [], vi.fn()))
    .toEqual({ expectedRevision: 3, fields: {} });
  expect(await buildMcpEdit(envelope, original, metadataDraft(), [], vi.fn()))
    .toEqual({ expectedRevision: 3, fields: {}, mcpMetadata: { action: "delete" } });
});

it("clears Codex options explicitly instead of silently reusing old values", async () => {
  const source = mcpEditSource(stdioEnvelope);
  if (source.server.type !== "stdio") throw new Error("stdio");
  delete source.server.codexOptions;
  const edit = await buildMcpEdit(stdioEnvelope, source, metadataDraft(), [], vi.fn());
  expect(edit.fields).toEqual({ codexOptions: { action: "delete" } });
});

it("validates metadata and every credential before starting credential writes", async () => {
  const put = vi.fn(async () => "new-reference");
  const source = parseMcpJson(JSON.stringify({ command: "run", env: { TOKEN: "sk-new-token", "BAD-NAME": "x" } }), "docs");
  await expect(buildMcpEdit(stdioEnvelope, source, metadataDraft(), [], put)).rejects.toThrow("环境变量名");
  const valid = parseMcpJson('{"command":"run","env":{"TOKEN":"sk-new-token"}}', "docs");
  await expect(buildMcpEdit(stdioEnvelope, valid, { ...metadataDraft(), docs: "file:///private" }, [], put)).rejects.toThrow("文档链接");
  expect(put).not.toHaveBeenCalled();
});

it("checks selected-client compatibility before writing a new secret", async () => {
  const source = parseMcpJson('{"type":"sse","url":"https://test.invalid","headers":{"X-Key":"sk-new-token"}}', "docs");
  const put = vi.fn();
  await expect(buildMcpEdit(stdioEnvelope, source, metadataDraft(), ["codex"], put)).rejects.toThrow("仅支持 Claude");
  expect(put).not.toHaveBeenCalled();
});

it("rejects duplicate new names but still allows editing the current name", async () => {
  const source = mcpEditSource(stdioEnvelope);
  const put = vi.fn();
  await expect(buildMcpEdit(stdioEnvelope, { ...source, name: "taken" }, metadataDraft(), [], put, ["docs", "taken"]))
    .rejects.toThrow("服务名称已存在");
  expect(await buildMcpEdit(stdioEnvelope, source, metadataDraft(), [], put, ["docs"]))
    .toEqual({ expectedRevision: 3, fields: {} });
});
