import { expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NewMcpForm } from "./NewMcpForm";
import type { AppKind, ExtensionDraft } from "../../api/client";
import type { NewMcpFormProps } from "./mcp-create/useMcpForm";
import { presetMetadata } from "./mcp-create/presets";

function renderForm(props: Partial<NewMcpFormProps> = {}) {
  const onSave = vi.fn(props.onSave ?? (async (_draft: ExtensionDraft, _clients: AppKind[]) => true));
  const onPutSecret = vi.fn(props.onPutSecret ?? (async () => "secret-abc"));
  render(<NewMcpForm {...props} busy={props.busy ?? false} onPutSecret={onPutSecret} onSave={onSave} />);
  return { onSave, onPutSecret };
}

function json(text: string) {
  fireEvent.change(screen.getByRole("textbox", { name: "MCP JSON 配置" }), { target: { value: text } });
}

function name(value: string) {
  fireEvent.change(screen.getByLabelText("服务名称"), { target: { value } });
}

const STDIO = { command: "npx", args: ["-y", "docs-server"], env: { MODE: "test" } };

it("starts with the JSON editor, two selected client icons and no paste-to-form path", () => {
  renderForm();
  expect(screen.getByRole("textbox", { name: "MCP JSON 配置" })).toHaveValue("");
  expect(screen.queryByLabelText("启动命令")).not.toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "保存后启用 Codex" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "保存后启用 Claude" })).toBeChecked();
  expect(screen.getAllByRole("checkbox").map((control) => control.getAttribute("aria-label")))
    .toEqual(["保存后启用 Claude", "保存后启用 Codex"]);
  expect(screen.getByRole("button", { name: "保存并启用" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "解析并填充到表单" })).not.toBeInTheDocument();
});

it.each([
  ["mcpServers", { mcpServers: { docs: STDIO } }],
  ["named", { docs: STDIO }],
  ["bare", STDIO],
])("saves %s JSON directly without visiting the wizard", async (_shape, spec) => {
  const { onSave, onPutSecret } = renderForm();
  name("docs");
  json(JSON.stringify(spec));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledExactlyOnceWith({ name: "docs", payload: {
    kind: "mcp", transport: "stdio", command: "npx", args: ["-y", "docs-server"],
    env: { MODE: { mode: "plain", value: "test" } },
  } }, ["codex", "claude"]);
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("applies the corrected time preset and keeps the JSON editable", async () => {
  renderForm();
  await userEvent.click(screen.getByRole("combobox", { name: "MCP 常用预设" }));
  await userEvent.click(screen.getByRole("option", { name: "time · 时间与时区" }));
  expect(screen.getByLabelText("服务名称")).toHaveValue("time");
  const editor = screen.getByRole("textbox", { name: "MCP JSON 配置" }) as HTMLTextAreaElement;
  expect(JSON.parse(editor.value)).toEqual({ type: "stdio", command: "uvx", args: ["mcp-server-time"], env: {} });
  json('{"command":"custom"}');
  expect(editor).toHaveValue('{"command":"custom"}');
  expect(screen.getByRole("combobox", { name: "MCP 常用预设" })).toHaveTextContent("自定义");
});

it("replaces the whole definition when selecting a preset, including stale HTTP credentials", async () => {
  const { onSave, onPutSecret } = renderForm();
  json(JSON.stringify({ old: { url: "https://old.test/mcp", headers: { Authorization: "old-secret" }, bearer: "old" } }));
  await userEvent.click(screen.getByRole("combobox", { name: "MCP 常用预设" }));
  await userEvent.click(screen.getByRole("option", { name: "fetch · 网页抓取" }));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith({ name: "fetch", mcpMetadata: presetMetadata("fetch"), payload: {
    kind: "mcp", transport: "stdio", command: "uvx", args: ["mcp-server-fetch"], env: {},
  } }, ["codex", "claude"]);
  expect(onPutSecret).not.toHaveBeenCalled();
});

it.each<AppKind[]>([["codex"], ["claude"], []])("saves only the selected clients: %j", async (...selected) => {
  const { onSave } = renderForm();
  json('{"docs":{"command":"uvx"}}');
  for (const client of ["codex", "claude"] as const) {
    if (!selected.includes(client)) {
      await userEvent.click(screen.getByRole("checkbox", { name: `保存后启用 ${client === "codex" ? "Codex" : "Claude"}` }));
    }
  }
  await userEvent.click(screen.getByRole("button", { name: selected.length ? "保存并启用" : "仅保存" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ name: "docs" }), selected);
});

it("keeps the visible name and a named JSON wrapper synchronized", async () => {
  const { onSave } = renderForm();
  json('{"mcpServers":{"first":{"command":"uvx"}}}');
  expect(screen.getByLabelText("服务名称")).toHaveValue("first");
  json('{"mcpServers":{"second":{"command":"uvx"}}}');
  expect(screen.getByLabelText("服务名称")).toHaveValue("second");
  name("renamed");
  expect(JSON.parse((screen.getByLabelText("MCP JSON 配置") as HTMLTextAreaElement).value))
    .toEqual({ mcpServers: { renamed: { command: "uvx" } } });
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ name: "renamed" }), ["codex", "claude"]);
});

it("rejects multiple servers before writing secrets or saving", async () => {
  const { onSave, onPutSecret } = renderForm();
  json('{"mcpServers":{"first":{"command":"uvx"},"second":{"command":"npx"}}}');
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("一次只能新建一个 MCP 服务");
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("retains all HTTP headers and Bearer environment references in the saved draft", async () => {
  const { onSave, onPutSecret } = renderForm();
  json(JSON.stringify({ docs: { type: "http", url: "https://docs.test/mcp", headers: {
    "X-Trace": "enabled", "X-Api-Key": "sk-new-key", "X-Host": "${MCP_HOST}",
  }, bearer: "${MCP_TOKEN}" } }));
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onPutSecret).toHaveBeenCalledExactlyOnceWith("sk-new-key", "mcp:docs:X-Api-Key");
  expect(onSave).toHaveBeenCalledWith({ name: "docs", payload: {
    kind: "mcp", transport: "http", url: "https://docs.test/mcp",
    headers: { "X-Trace": { mode: "plain", value: "enabled" }, "X-Api-Key": { mode: "secretRef", reference: "secret-abc" },
      "X-Host": { mode: "envRef", name: "MCP_HOST" } }, bearer: { mode: "envRef", name: "MCP_TOKEN" },
  } }, ["codex", "claude"]);
});

it("does not reenter onSave while its promise is pending, even when busy stays false", async () => {
  let finish!: (value: boolean) => void;
  const { onSave } = renderForm({ onSave: () => new Promise((resolve) => { finish = resolve; }) });
  json('{"docs":{"command":"uvx"}}');
  const form = screen.getByRole("form", { name: "新建 MCP 服务" });
  fireEvent.submit(form);
  fireEvent.submit(form);
  await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
  expect(screen.getByRole("button", { name: "保存中" })).toBeDisabled();
  expect(screen.getByLabelText("MCP JSON 配置")).toBeDisabled();
  finish(true);
  await waitFor(() => expect(screen.getByRole("button", { name: "保存并启用" })).toBeEnabled());
});

it("locks the entire save operation before the credential promise resolves", async () => {
  let finish!: (value: string | null) => void;
  const { onSave, onPutSecret } = renderForm({ onPutSecret: () => new Promise((resolve) => { finish = resolve; }) });
  json('{"docs":{"command":"uvx","env":{"TOKEN":"sk-new-key"}}}');
  const form = screen.getByRole("form", { name: "新建 MCP 服务" });
  fireEvent.submit(form);
  fireEvent.submit(form);
  expect(onPutSecret).toHaveBeenCalledTimes(1);
  expect(onSave).not.toHaveBeenCalled();
  finish("new-reference");
  await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
});

it("preserves the document and allows retry after a failed save", async () => {
  const { onSave } = renderForm({ onSave: async () => false });
  const text = '{"docs":{"command":"uvx"}}';
  json(text);
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("配置已保留");
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(text);
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(onSave).toHaveBeenCalledTimes(2);
});

it("does not save when a credential write fails and preserves the JSON", async () => {
  const { onSave } = renderForm({ onPutSecret: async () => null });
  const text = '{"docs":{"command":"uvx","env":{"TOKEN":"sk-new-key"}}}';
  json(text);
  await userEvent.click(screen.getByRole("button", { name: "保存并启用" }));
  expect(screen.getByRole("alert")).toHaveTextContent("凭据保存失败");
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(text);
  expect(onSave).not.toHaveBeenCalled();
});

it("honors the external busy state even for direct form submission", () => {
  const { onSave, onPutSecret } = renderForm({ busy: true });
  fireEvent.submit(screen.getByRole("form", { name: "新建 MCP 服务" }));
  expect(screen.getByRole("button", { name: "配置向导" })).toBeDisabled();
  expect(screen.getByRole("checkbox", { name: "保存后启用 Codex" })).toBeDisabled();
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});
