import { describe, expect, it } from "vitest";
import { formatMcpJson, parseMcpJson } from "./mcp-json";

describe("strict single-server JSON", () => {
  it.each([
    ["null", "必须是 JSON 对象"],
    ["[]", "必须是 JSON 对象"],
    ["true", "必须是 JSON 对象"],
    ["{", "JSON 语法错误"],
    ["{}", "检测到 0 项"],
    ['{"mcpServers":{}}', "检测到 0 项"],
    ['{"mcpServers":[]}', "必须是 JSON 对象"],
    ['{"mcpServers":{"one":{"command":"run"},"two":{"command":"run"}}}', "检测到 2 项"],
    ['{"one":{"command":"run"},"two":{"command":"run"}}', "检测到 2 项"],
    ['{"mcpServers":{"one":{"command":"run"}},"other":{}}', "不支持的字段：other"],
    ['{"one":null}', "必须是 JSON 对象"],
    ['{"command":42}', "command 必须是字符串"],
    ['{"command":"run","args":"-y package"}', "args 必须是字符串数组"],
    ['{"command":"run","args":["-y",3]}', "args[1] 必须是字符串"],
    ['{"command":"run","env":[]}', "env 必须是 JSON 对象"],
    ['{"command":"run","env":{"PORT":10}}', "env.PORT 必须是 JSON 对象"],
    ['{"command":"run","url":"https://test.invalid"}', "不能同时出现"],
    ['{"type":"http","command":"run"}', "不支持的字段：command"],
    ['{"type":"stdio","url":"https://test.invalid"}', "不支持的字段：url"],
    ['{"type":"HTTP","url":"https://test.invalid"}', "type 仅支持"],
    ['{"type":false,"command":"run"}', "type 仅支持"],
    ['{"type":"unknown","command":"run"}', "type 仅支持"],
    ['{"command":"run","disabled":true}', "不支持的字段：disabled"],
    ['{"command":"run","cwd":false}', "工作目录"],
    ['{"command":"run","timeout":10}', "不支持的字段：timeout"],
    ['{"url":true}', "url 必须是字符串"],
    ['{"url":"https://test.invalid","env":{}}', "不支持的字段：env"],
    ['{"url":"https://test.invalid","headers":null}', "headers 必须是 JSON 对象"],
    ['{"url":"https://test.invalid","headers":{"X-Trace":false}}', "headers.X-Trace 必须是 JSON 对象"],
    ['{"url":"https://test.invalid","bearer":42}', "bearer 必须是 JSON 对象"],
    ['{"type":"sse","url":"https://test.invalid","bearer":"token"}', "不支持的字段：bearer"],
  ])("rejects unsupported or malformed input %s", (text, error) => {
    expect(() => parseMcpJson(text, "test")).toThrow(error);
  });

  it("round-trips all arguments, including whitespace, empty strings and embedded line breaks", () => {
    const args = ["-y", "package name", " leading ", "", "two\nlines", "\r\n"];
    const source = parseMcpJson(JSON.stringify({ command: "run", args, env: { EMPTY: "", TOKEN: "", MULTILINE: "a\nb" } }), "test");
    expect(source.server).toMatchObject({ args, env: { EMPTY: { mode: "plain", value: "" }, TOKEN: { mode: "plain", value: "" } } });
    expect(parseMcpJson(formatMcpJson(source.server), source.name)).toEqual(source);
  });

  it("preserves HTTP values and decodes exact environment-variable references", () => {
    const source = parseMcpJson(JSON.stringify({ remote: {
      type: "http", url: "https://mcp.test/path?query=value", bearer: "${ACCESS_TOKEN}",
      headers: { Accept: "text/event-stream", "X-Tenant": "${TENANT}", AuthorizationKey: "new-token" },
    } }));
    expect(source).toEqual({ name: "remote", server: {
      type: "http", url: "https://mcp.test/path?query=value",
      headers: { Accept: { mode: "plain", value: "text/event-stream" }, "X-Tenant": { mode: "envRef", name: "TENANT" },
        AuthorizationKey: { mode: "secret", value: "new-token" } },
      bearer: { mode: "envRef", name: "ACCESS_TOKEN" },
    } });
    expect(parseMcpJson(formatMcpJson(source.server), source.name)).toEqual(source);
  });

  it.each(["secretRef", "stored", "redacted", "secretConfigured"])("does not accept existing %s credentials", (mode) => {
    const text = JSON.stringify({ command: "run", env: { TOKEN: { mode, reference: "existing-private-handle" } } });
    expect(() => parseMcpJson(text, "test")).toThrow("不能导入已存凭据引用");
    expect(() => parseMcpJson(text, "test")).not.toThrow("existing-private-handle");
  });

  it.each([
    { mode: "envRef", name: "TOKEN", extra: true },
    { mode: "envRef", name: 3 },
    { mode: "plain", value: false },
    { mode: "secret", value: "new-token", extra: true },
    { mode: "other", value: "ignored" },
  ])("rejects invalid credential shapes %j", (value) => {
    expect(() => parseMcpJson(JSON.stringify({ command: "run", env: { TOKEN: value } }), "test")).toThrow();
  });
});
