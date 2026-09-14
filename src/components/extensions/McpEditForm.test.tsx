import { expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { McpEditForm, type McpEditFormProps } from "./McpEditForm";
import { httpEnvelope, stdioEnvelope } from "./mcp-edit/test-fixtures";

function renderForm(props: Partial<McpEditFormProps> = {}) {
  const onSave = vi.fn(props.onSave ?? (async () => true));
  const onPutSecret = vi.fn(props.onPutSecret ?? (async () => "new-handle"));
  const onCancel = vi.fn();
  const onBusyChange = vi.fn();
  const args = { envelope: stdioEnvelope, busy: false, ...props, onSave, onPutSecret, onCancel, onBusyChange };
  const rendered = render(<McpEditForm {...args} />);
  return { ...rendered, args, onSave, onPutSecret, onCancel, onBusyChange };
}

function config() { return JSON.parse((screen.getByLabelText("MCP JSON 配置") as HTMLTextAreaElement).value); }
function change(label: string, value: string) { fireEvent.change(screen.getByLabelText(label), { target: { value } }); }
function json(value: unknown) { change("MCP JSON 配置", JSON.stringify(value)); }
async function save() { await userEvent.click(screen.getByRole("button", { name: "保存修改" })); }

it("starts JSON-first with complete options and stored-secret presence markers, not the obsolete form", () => {
  renderForm();
  expect(config()).toMatchObject({ type: "stdio", command: "npx", args: stdioEnvelope.args,
    env: { DOCS_TOKEN: { mode: "secretConfigured" } }, codexOptions: stdioEnvelope.codexOptions });
  expect(screen.queryByLabelText("启动命令")).not.toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "传输方式" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "配置向导" })).toBeEnabled();
  expect(screen.queryByDisplayValue("new-handle")).not.toBeInTheDocument();
});

it("submits a no-op without mentioning or re-storing any retained credentials", async () => {
  const { onSave, onPutSecret, onCancel } = renderForm();
  await save();
  expect(onSave).toHaveBeenCalledExactlyOnceWith({ expectedRevision: 3, fields: {} }, []);
  expect(onPutSecret).not.toHaveBeenCalled();
  expect(onCancel).toHaveBeenCalledOnce();
});

it("changes only touched fields and round-trips whitespace, empty arguments and all Codex options", async () => {
  const { onSave, onPutSecret } = renderForm();
  json({ ...config(), command: "docker" });
  await save();
  expect(onSave).toHaveBeenCalledWith({ expectedRevision: 3, fields: { command: { action: "replace", value: "docker" } } }, []);
  expect(onPutSecret).not.toHaveBeenCalled();
  expect(JSON.stringify(onSave.mock.calls)).not.toContain("DOCS_TOKEN");
});

it("removes a stored environment credential only when its JSON slot is explicitly deleted", async () => {
  const { onSave } = renderForm();
  const next = config();
  delete next.env.DOCS_TOKEN;
  json(next);
  await save();
  expect(onSave.mock.calls[0][0].fields?.env).toEqual({ DOCS_TOKEN: { action: "delete" } });
});

it("stores replacement credentials before sending only their new handles", async () => {
  const { onSave, onPutSecret } = renderForm();
  json({ ...config(), env: { ...config().env, DOCS_TOKEN: "sk-new-token" } });
  await save();
  expect(onPutSecret).toHaveBeenCalledExactlyOnceWith("sk-new-token", "mcp:docs:DOCS_TOKEN");
  expect(onSave.mock.calls[0][0].fields?.env).toEqual({ DOCS_TOKEN: { action: "replace", value: { mode: "secretRef", reference: "new-handle" } } });
  expect(JSON.stringify(onSave.mock.calls)).not.toContain("sk-new-token");
});

it.each(["keep", "remove", "replace"])("supports %s of an existing HTTP bearer without exposing it", async (operation) => {
  const { onSave, onPutSecret } = renderForm({ envelope: httpEnvelope });
  const next = config();
  if (operation === "remove") delete next.bearer;
  if (operation === "replace") next.bearer = { mode: "envRef", name: "HTTP_TOKEN" };
  json(next);
  await save();
  const fields = onSave.mock.calls[0][0].fields;
  expect(fields?.headers).toBeUndefined();
  expect(fields?.bearer).toEqual(operation === "keep" ? undefined : operation === "remove" ? { action: "delete" }
    : { action: "replace", value: { mode: "envRef", name: "HTTP_TOKEN" } });
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("sends the renamed key alongside a complete transport replacement", async () => {
  const { onSave } = renderForm();
  json({ remote: { type: "ws", url: "wss://mcp.test/ws", headers: { "X-Mode": "test" } } });
  await save();
  expect(onSave).toHaveBeenCalledWith({ expectedRevision: 3, serverKey: "remote", fields: {}, transport: {
    transport: "claudeWs", url: "wss://mcp.test/ws", headers: { "X-Mode": { mode: "plain", value: "test" } },
  } }, []);
});

it("prefills the optional wizard and applies it back to JSON without losing secrets or option values", async () => {
  const { onSave, onPutSecret } = renderForm();
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  stdioEnvelope.args.forEach((value, index) => expect(screen.getByLabelText("启动参数 " + (index + 1))).toHaveValue(value));
  expect(screen.getByText("已设置凭据（保持不变）")).toBeInTheDocument();
  expect(screen.getByLabelText("环境变量名 2")).toBeDisabled();
  expect(screen.getByLabelText("启动超时（秒）")).toHaveValue(0);
  expect(screen.getByRole("combobox", { name: "Codex required" })).toHaveTextContent("否");
  change("启动命令", "docker");
  await userEvent.click(screen.getByRole("button", { name: "应用配置" }));
  expect(config()).toMatchObject({ command: "docker", args: stdioEnvelope.args, codexOptions: stdioEnvelope.codexOptions });
  expect(config().env.DOCS_TOKEN).toEqual({ mode: "secretConfigured" });
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
  await save();
  expect(onSave.mock.calls[0][0].fields).toEqual({ command: { action: "replace", value: "docker" } });
});

it("cancels wizard changes back to the same JSON document and focuses the editor", async () => {
  const { onSave, onPutSecret } = renderForm();
  const before = (screen.getByLabelText("MCP JSON 配置") as HTMLTextAreaElement).value;
  await userEvent.click(screen.getByRole("button", { name: "配置向导" }));
  change("启动命令", "discard");
  fireEvent.keyDown(screen.getByLabelText("启动命令"), { key: "Escape" });
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveValue(before);
  expect(screen.getByLabelText("MCP JSON 配置")).toHaveFocus();
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("edits metadata independently from the client server key", async () => {
  const { onSave } = renderForm({ envelope: { ...stdioEnvelope, mcpMetadata: {
    displayName: "Docs Search", description: "Existing", tags: ["docs"], homepage: "https://example.test", docs: "https://example.test/docs",
  } } });
  change("显示名称", "New display");
  change("描述", "");
  change("标签", "docs, search, docs");
  change("主页", "");
  change("文档链接", "https://example.test/new");
  await save();
  expect(onSave.mock.calls[0][0]).toEqual({ expectedRevision: 3, fields: {}, mcpMetadata: { action: "replace", value: {
    displayName: "New display", tags: ["docs", "search"], docs: "https://example.test/new",
  } } });
});

it("validates the whole document before any credentials are written", async () => {
  const { onSave, onPutSecret } = renderForm();
  json({ ...config(), env: { TOKEN: "sk-new-token" }, codexOptions: { toolTimeoutSec: -1 } });
  await save();
  expect(screen.getByRole("alert")).toHaveTextContent("工具超时");
  expect(onPutSecret).not.toHaveBeenCalled();
  expect(onSave).not.toHaveBeenCalled();
});

it("locks synchronously across both secret writes and the save callback", async () => {
  let finishSecret!: (value: string) => void;
  let finishSave!: (value: boolean) => void;
  const { onSave, onPutSecret, onBusyChange } = renderForm({
    onPutSecret: () => new Promise((resolve) => { finishSecret = resolve; }),
    onSave: () => new Promise((resolve) => { finishSave = resolve; }),
  });
  json({ ...config(), env: { ...config().env, DOCS_TOKEN: "sk-new-token" } });
  const form = screen.getByRole("form", { name: "编辑 MCP 服务" });
  fireEvent.submit(form);
  fireEvent.submit(form);
  expect(onPutSecret).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("MCP JSON 配置")).toBeDisabled();
  expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
  await act(async () => finishSecret("new-handle"));
  fireEvent.submit(form);
  expect(onSave).toHaveBeenCalledTimes(1);
  expect(onBusyChange).toHaveBeenCalledExactlyOnceWith(true);
  await act(async () => finishSave(false));
  expect(screen.getByRole("alert")).toHaveTextContent("配置已保留");
  expect(screen.getByRole("button", { name: "保存修改" })).toBeEnabled();
  expect(onBusyChange).toHaveBeenLastCalledWith(false);
});

it("preserves the draft after failures and releases the save lock for a retry", async () => {
  const { onSave, onCancel } = renderForm({ onSave: async () => { throw new Error("扩展已被其他窗口修改"); } });
  json({ ...config(), command: "updated" });
  await save();
  expect(screen.getByRole("alert")).toHaveTextContent("扩展已被其他窗口修改");
  expect(config().command).toBe("updated");
  await save();
  expect(onSave).toHaveBeenCalledTimes(2);
  expect(onCancel).not.toHaveBeenCalled();
});

it("honors external busy for direct submissions as well as disabled buttons", () => {
  const { onSave, onPutSecret } = renderForm({ busy: true });
  fireEvent.submit(screen.getByRole("form", { name: "编辑 MCP 服务" }));
  expect(onSave).not.toHaveBeenCalled();
  expect(onPutSecret).not.toHaveBeenCalled();
});

it("passes selected clients to the outer callback without writing client configs itself", async () => {
  const { onSave } = renderForm({ initialClients: ["codex", "claude"] });
  await userEvent.click(screen.getByRole("checkbox", { name: "保存后启用 Codex" }));
  await save();
  expect(onSave).toHaveBeenCalledWith({ expectedRevision: 3, fields: {} }, ["claude"]);
});

it("reloads a changed definition revision instead of reusing a stale JSON draft", async () => {
  const view = renderForm();
  json({ ...config(), command: "unsaved" });
  view.rerender(<McpEditForm {...view.args} envelope={{ ...stdioEnvelope, revision: 4, command: "new-revision" }} />);
  expect(config().command).toBe("new-revision");
  await save();
  await waitFor(() => expect(view.onSave).toHaveBeenCalledWith({ expectedRevision: 4, fields: {} }, []));
});
