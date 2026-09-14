import { expect, it, vi } from "vitest";
import { materializeMcpDraft, validateMcpSource } from "./mcp-draft";
import { parseMcpJson } from "./mcp-json";

it.each([
  [{ command: "" }, "stdio 启动命令不能为空"],
  [{ command: "run", env: { "BAD-NAME": "value" } }, "环境变量名"],
  [{ command: "run", env: { TOKEN: { mode: "envRef", name: "bad-name" } } }, "环境变量引用名称不合法"],
  [{ command: "run", env: { TOKEN: { mode: "plain", value: "sk-new-token" } } }, "疑似凭据"],
  [{ command: "run", env: { TOKEN: { mode: "secret", value: "" } } }, "凭据不能为空"],
  [{ url: "relative/path" }, "有效的完整 URL"],
  [{ url: "file:///etc/config" }, "必须使用"],
  [{ url: "https://test.invalid", headers: { "Bad:Name": "value" } }, "请求头名称"],
  [{ url: "https://test.invalid", headers: { "X-Mode": "1", "x-mode": "2" } }, "重复"],
  [{ url: "https://test.invalid", headers: { "X-Mode": "first\r\nsecond" } }, "请求头换行"],
  [{ url: "https://test.invalid", headers: { authorization: "token" }, bearer: "${TOKEN}" }, "不能同时设置"],
])("validates the complete configuration before any writes: %j", (spec, message) => {
  const source = parseMcpJson(JSON.stringify(spec), "test");
  expect(() => validateMcpSource(source, [])).toThrow(message as string);
});

it.each(["", "with space", "non/ascii", "服务", "x".repeat(65)])("rejects invalid service name %s", (name) => {
  expect(() => validateMcpSource(parseMcpJson('{"command":"run"}', name), [])).toThrow("服务名称");
});

it("refuses Codex environment aliases while retaining the valid Claude definition", () => {
  const source = parseMcpJson('{"command":"run","env":{"API_TOKEN":"${HOST_TOKEN}"}}', "test");
  expect(() => validateMcpSource(source, ["codex", "claude"])).toThrow("不支持环境变量改名映射");
  expect(() => validateMcpSource(source, ["claude"])).not.toThrow();
});

it("requires an environment reference for Codex Bearer without silently dropping it", async () => {
  const source = parseMcpJson('{"url":"https://mcp.test","bearer":"new-token"}', "test");
  expect(() => validateMcpSource(source, ["codex"])).toThrow("Bearer 仅支持环境变量引用");
  expect(() => validateMcpSource(source, ["claude"])).not.toThrow();
  const putSecret = vi.fn(async () => "new-reference");
  expect(await materializeMcpDraft(source, putSecret)).toEqual({ name: "test", payload: {
    kind: "mcp", transport: "http", url: "https://mcp.test", headers: {},
    bearer: { mode: "secretRef", reference: "new-reference" },
  } });
  expect(putSecret).toHaveBeenCalledExactlyOnceWith("new-token", "mcp:test:bearer");
});

it.each([["sse", "claudeSse", "https://mcp.test"], ["ws", "claudeWs", "wss://mcp.test"]])(
  "preserves %s transport and enforces its supported client", async (type, transport, url) => {
    const source = parseMcpJson(JSON.stringify({ type, url, headers: { "X-Mode": "test" } }), "test");
    expect(() => validateMcpSource(source, ["codex"])).toThrow("仅支持 Claude");
    expect(() => validateMcpSource(source, ["claude"])).not.toThrow();
    expect(await materializeMcpDraft(source, vi.fn())).toEqual({ name: "test", payload: {
      kind: "mcp", transport, url, headers: { "X-Mode": { mode: "plain", value: "test" } },
    } });
  },
);

it("converts each new credential independently and never passes its literal to onSave", async () => {
  const source = parseMcpJson(JSON.stringify({ command: "run", args: [" a b ", ""], env: {
    TOKEN: "sk-new-token", FLAG: "enabled", INHERITED: "${INHERITED}",
  } }), "test");
  const putSecret = vi.fn(async () => "stored-reference");
  const draft = await materializeMcpDraft(source, putSecret);
  expect(draft).toEqual({ name: "test", payload: {
    kind: "mcp", transport: "stdio", command: "run", args: [" a b ", ""], env: {
      TOKEN: { mode: "secretRef", reference: "stored-reference" }, FLAG: { mode: "plain", value: "enabled" },
      INHERITED: { mode: "envRef", name: "INHERITED" },
    },
  } });
  expect(JSON.stringify(draft)).not.toContain("sk-new-token");
  expect(putSecret).toHaveBeenCalledExactlyOnceWith("sk-new-token", "mcp:test:TOKEN");
});
