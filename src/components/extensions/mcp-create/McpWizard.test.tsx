import { expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NewMcpForm } from "../NewMcpForm";

function renderSource(json = "") {
  const onSave = vi.fn(async () => true);
  const onPutSecret = vi.fn(async () => "stored-new-secret");
  render(<NewMcpForm busy={false} onSave={onSave} onPutSecret={onPutSecret} />);
  fireEvent.change(screen.getByLabelText("MCP JSON 配置"), { target: { value: json } });
  return { onSave, onPutSecret };
}

function change(label: string, value: string) {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
}

it("cancels wizard edits without changing the original JSON, name or client choices", async () => {
  const text = '{"remote":{"url":"https://original.test","headers":{"Authorization":"new-token","X-Trace":"yes"}}}';
  const { onSave, onPutSecret } = renderSource(text);
  await userEvent.click(screen.getByRole("checkbox", { name: "保存后启用 Codex" }));
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  expect(screen.getByLabelText("请求头 1 值")).toHaveValue("new-token");
  change("服务地址", "https://discarded.test");
  change("服务名称", "discarded");
  await userEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(text);
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveFocus();
  expect(screen.getByLabelText("服务名称")).toHaveValue("remote");
  expect(screen.getByRole("checkbox", { name: "保存后启用 Codex" })).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "保存后启用 Claude" })).toBeChecked();
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("cancels back to JSON with Escape rather than dropping the new-server document", async () => {
  const text = '{"stdio":{"command":"uvx"}}';
  renderSource(text);
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  change("启动命令", "discarded");
  fireEvent.keyDown(screen.getByLabelText("启动命令"), { key: "Escape" });
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(text);
});

it("round-trips whitespace, empty and multiline arguments through the wizard without splitting", async () => {
  const args = ["-y", "package name", " leading and trailing ", "", "line one\nline two"];
  const { onSave } = renderSource(JSON.stringify({ docs: { command: "npx", args, env: { EMPTY: "", MULTILINE: "a\nb" } } }));
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  args.forEach((value, index) => expect(screen.getByLabelText(`启动参数 ${index + 1}`)).toHaveValue(value));
  change("启动参数 2", "changed package");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith({ name: "docs", payload: {
    kind: "mcp", transport: "stdio", command: "npx", args: ["-y", "changed package", ...args.slice(2)],
    env: { EMPTY: { mode: "plain", value: "" }, MULTILINE: { mode: "plain", value: "a\nb" } },
  } }, ["codex", "claude"]);
});

it("creates an HTTP definition in the wizard with header and bearer credentials intact", async () => {
  const { onSave, onPutSecret } = renderSource();
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  change("服务名称", "remote");
  await userEvent.click(screen.getByRole("combobox", { name: "传输方式" }));
  await userEvent.click(screen.getByRole("option", { name: "HTTP · 远程端点" }));
  change("服务地址", "https://remote.test/mcp");
  await userEvent.click(screen.getByRole("button", { name: "添加请求头" }));
  change("请求头名 1", "X-Key");
  await userEvent.click(screen.getByRole("combobox", { name: "请求头 1 值类型" }));
  await userEvent.click(screen.getByRole("option", { name: "系统凭据" }));
  change("请求头 1 值", "new-header-secret");
  await userEvent.click(screen.getByRole("checkbox", { name: "Bearer 凭据" }));
  change("Bearer 凭据值", "MCP_BEARER");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  expect(onPutSecret).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onPutSecret).toHaveBeenCalledExactlyOnceWith("new-header-secret", "mcp:remote:X-Key");
  expect(onSave).toHaveBeenCalledWith({ name: "remote", payload: {
    kind: "mcp", transport: "http", url: "https://remote.test/mcp",
    headers: { "X-Key": { mode: "secretRef", reference: "stored-new-secret" } },
    bearer: { mode: "envRef", name: "MCP_BEARER" },
  } }, ["codex", "claude"]);
});

it("can cancel a destructive transport change, then explicitly replace all transport-specific fields", async () => {
  const text = '{"docs":{"command":"uvx","args":["server"],"env":{"TOKEN":"sk-new-secret"}}}';
  const { onSave, onPutSecret } = renderSource(text);
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  await userEvent.click(screen.getByRole("combobox", { name: "传输方式" }));
  await userEvent.click(screen.getByRole("option", { name: "HTTP · 远程端点" }));
  expect(screen.getByRole("alert")).toHaveTextContent("清除不适用的字段");
  await userEvent.click(screen.getByRole("button", { name: "取消更改" }));
  expect(screen.getByLabelText("启动参数 1")).toHaveValue("server");
  expect(screen.getByLabelText("环境变量 1 值")).toHaveValue("sk-new-secret");
  await userEvent.click(screen.getByRole("combobox", { name: "传输方式" }));
  await userEvent.click(screen.getByRole("option", { name: "HTTP · 远程端点" }));
  await userEvent.click(screen.getByRole("button", { name: "更改传输方式" }));
  change("服务地址", "https://new.test/mcp");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith({ name: "docs", payload: {
    kind: "mcp", transport: "http", url: "https://new.test/mcp", headers: {},
  } }, ["codex", "claude"]);
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("uses the current JSON on every wizard entry, without retaining an earlier credential state", async () => {
  renderSource('{"old":{"url":"https://old.test","bearer":"${OLD_TOKEN}"}}');
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  expect(screen.getByLabelText("Bearer 凭据值")).toHaveValue("OLD_TOKEN");
  await userEvent.click(screen.getByRole("button", { name: "取消" }));
  change("MCP JSON 配置", '{"new":{"url":"https://new.test","headers":{"X-Trace":"fresh"}}}');
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  expect(screen.getByLabelText("服务名称")).toHaveValue("new");
  expect(screen.getByLabelText("服务地址")).toHaveValue("https://new.test");
  expect(screen.getByRole("checkbox", { name: "Bearer 凭据" })).not.toBeChecked();
  expect(screen.queryByLabelText("Bearer 凭据值")).not.toBeInTheDocument();
  expect(screen.getByLabelText("请求头 1 值")).toHaveValue("fresh");
});

it("refuses to enter the wizard if JSON has fields it cannot represent", async () => {
  const text = '{"docs":{"command":"run","unknown":"must-not-disappear"}}';
  const { onSave } = renderSource(text);
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  expect(screen.getByRole("alert")).toHaveTextContent("不支持的字段：unknown");
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(text);
  expect(screen.queryByRole("form", { name: "MCP 配置向导" })).not.toBeInTheDocument();
  expect(onSave).not.toHaveBeenCalled();
});

it("rejects duplicate credential names instead of overwriting the first row", async () => {
  const { onSave, onPutSecret } = renderSource('{"docs":{"command":"run","env":{"VALUE":"first"}}}');
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  await userEvent.click(screen.getByRole("button", { name: "添加环境变量" }));
  change("环境变量名 2", "VALUE");
  change("环境变量 2 值", "second");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  expect(screen.getByRole("alert")).toHaveTextContent("环境变量 VALUE 重复");
  expect(screen.getByLabelText("环境变量 1 值")).toHaveValue("first");
  expect(screen.getByLabelText("环境变量 2 值")).toHaveValue("second");
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});
