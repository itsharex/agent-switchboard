import { expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NewMcpForm } from "./NewMcpForm";

function change(label: string, value: string) { fireEvent.change(screen.getByLabelText(label), { target: { value } }); }
function form(existingNames: string[] = []) {
  const onSave = vi.fn(async () => true);
  const onPutSecret = vi.fn(async () => "new-handle");
  render(<NewMcpForm busy={false} existingNames={existingNames} onSave={onSave} onPutSecret={onPutSecret} />);
  change("MCP JSON 配置", '{"docs":{"command":"run"}}');
  return { onSave, onPutSecret };
}

it("saves optional display metadata without changing the native server key", async () => {
  const { onSave } = form();
  change("显示名称", " Docs Search ");
  await userEvent.click(screen.getByText("附加信息"));
  change("描述", " Search documentation ");
  change("标签", "docs, search, docs, ");
  change("主页", "https://example.test");
  change("文档链接", "https://example.test/docs");
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith({ name: "docs", mcpMetadata: { displayName: "Docs Search", description: "Search documentation",
    tags: ["docs", "search"], homepage: "https://example.test", docs: "https://example.test/docs" },
    payload: { kind: "mcp", transport: "stdio", command: "run", args: [], env: {} } }, ["codex", "claude"]);
  expect(screen.getByLabelText("显示名称")).toHaveValue("");
});

it("validates metadata before creating any secret references", async () => {
  const { onSave, onPutSecret } = form();
  change("MCP JSON 配置", '{"docs":{"command":"run","env":{"TOKEN":"sk-new-secret"}}}');
  await userEvent.click(screen.getByText("附加信息"));
  change("文档链接", "javascript:invalid");
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("文档链接");
  expect(onPutSecret).not.toHaveBeenCalled();
  expect(onSave).not.toHaveBeenCalled();
});

it("saves all Codex options through the add wizard without losing false or zero", async () => {
  const { onSave } = form();
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  change("工作目录", "C:/tools/docs");
  change("启动超时（秒）", "0");
  change("工具超时（秒）", "45");
  await userEvent.click(screen.getByRole("combobox", { name: "Codex required" }));
  await userEvent.click(screen.getByRole("option", { name: "否" }));
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ payload: expect.objectContaining({
    codexOptions: { cwd: "C:/tools/docs", startupTimeoutSec: 0, toolTimeoutSec: 45, required: false },
  }) }), ["codex", "claude"]);
});

it("blocks duplicate service names and picks a unique preset name", async () => {
  const { onSave, onPutSecret } = form(["docs", "fetch", "fetch-1"]);
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("服务名称已存在");
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("combobox", { name: "MCP 常用预设" }));
  await userEvent.click(screen.getByRole("option", { name: "fetch · 网页抓取" }));
  expect(screen.getByLabelText("服务名称")).toHaveValue("fetch-2");
  expect(screen.getByLabelText("显示名称")).toHaveValue("mcp-server-fetch");
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ name: "fetch-2", mcpMetadata: expect.objectContaining({ tags: ["stdio", "http", "web"] }) }), ["codex", "claude"]);
});

it("preserves metadata while applying or cancelling the configuration wizard", async () => {
  form();
  change("显示名称", "Display only");
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  change("启动命令", "updated");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  expect(screen.getByLabelText("显示名称")).toHaveValue("Display only");
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  await userEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(screen.getByLabelText("显示名称")).toHaveValue("Display only");
});
