import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { McpEditForm } from "./McpEditForm";
import type { McpEditViewEnvelope } from "../../api/client";

const stdioEnvelope: McpEditViewEnvelope = {
  id: "ext-mcp-1",
  revision: 3,
  name: "docs",
  transport: "stdio",
  command: "npx",
  args: ["-y", "docs-server"],
  env: [
    { name: "LOG_LEVEL", value: { mode: "plain", value: "debug" } },
    { name: "DOCS_TOKEN", value: { mode: "secretConfigured" } },
  ],
  codexOptions: { startupTimeoutSec: 30 },
};

const httpEnvelope: McpEditViewEnvelope = {
  id: "ext-mcp-2",
  revision: 5,
  name: "api",
  transport: "http",
  url: "https://mcp.example.test/v1",
  headers: [{ name: "X-Api-Key", value: { mode: "secretConfigured" } }],
  bearer: { mode: "secretConfigured" },
};

function renderForm(envelope: McpEditViewEnvelope) {
  const onSave = vi.fn().mockResolvedValue(true);
  const onPutSecret = vi.fn().mockResolvedValue("secret-new");
  const onCancel = vi.fn();
  const view = render(
    <McpEditForm
      envelope={envelope}
      busy={false}
      onPutSecret={onPutSecret}
      onSave={onSave}
      onCancel={onCancel}
    />,
  );
  return { onSave, onPutSecret, onCancel, unmount: view.unmount };
}

async function save(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "保存修改" }));
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("McpEditForm", () => {
  it("prefills every editable field and marks stored credentials without leaking them", () => {
    renderForm(stdioEnvelope);

    expect(screen.getByLabelText("启动命令")).toHaveValue("npx");
    expect(screen.getByLabelText("启动参数")).toHaveValue("-y\ndocs-server");
    expect(screen.getByLabelText("环境变量名 1")).toHaveValue("LOG_LEVEL");
    expect(screen.getByLabelText("环境变量值 1")).toHaveValue("debug");
    expect(screen.getByText("已设置凭据（保持不变）")).toBeInTheDocument();
    expect(screen.getByLabelText("启动超时（秒）")).toHaveValue(30);
    // The stored secret's handle or value never appears anywhere.
    expect(screen.queryByDisplayValue(/secret/i)).not.toBeInTheDocument();
  });

  it("sends an empty patch when nothing changed", async () => {
    const user = userEvent.setup();
    const { onSave, onCancel } = renderForm(stdioEnvelope);

    await save(user);

    expect(onSave).toHaveBeenCalledWith({ expectedRevision: 3, fields: {} });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("replaces only the touched field and keeps the stored credential unnamed", async () => {
    const user = userEvent.setup();
    const { onSave } = renderForm(stdioEnvelope);

    const command = screen.getByLabelText("启动命令");
    await user.clear(command);
    await user.type(command, "docker");
    await save(user);

    const request = onSave.mock.calls[0][0];
    expect(request.fields.command).toEqual({ action: "replace", value: "docker" });
    // The untouched plain slot and the stored credential are absent.
    expect(request.fields.env).toBeUndefined();
    expect(JSON.stringify(request)).not.toContain("DOCS_TOKEN");
    expect(JSON.stringify(request)).not.toContain("secret");
  });

  it("removes an initial slot as a delete edit", async () => {
    const user = userEvent.setup();
    const { onSave } = renderForm(stdioEnvelope);

    await user.click(screen.getByRole("button", { name: "移除环境变量 1" }));
    await save(user);

    expect(onSave.mock.calls[0][0].fields.env).toEqual({
      LOG_LEVEL: { action: "delete" },
    });
  });

  it("stores a new secret before referencing it in the patch", async () => {
    const user = userEvent.setup();
    const { onSave, onPutSecret } = renderForm(stdioEnvelope);

    await user.click(screen.getByRole("button", { name: "添加环境变量" }));
    const index = screen.getAllByLabelText(/环境变量名/).length;
    await user.type(screen.getByLabelText(`环境变量名 ${index}`), "TRACE_ID");
    await user.click(screen.getByRole("combobox", { name: `环境变量 ${index} 值类型` }));
    await user.click(screen.getByRole("option", { name: "新凭据（存入系统凭据）" }));
    await user.type(screen.getByLabelText(`环境变量值 ${index}`), "trace-secret-value");
    await save(user);

    expect(onPutSecret).toHaveBeenCalledWith(
      "trace-secret-value",
      expect.stringContaining("TRACE_ID"),
    );
    expect(onSave.mock.calls[0][0].fields.env).toEqual({
      TRACE_ID: { action: "replace", value: { mode: "secretRef", reference: "secret-new" } },
    });
  });

  it("keeps, removes, or replaces the http bearer as one position", async () => {
    const user = userEvent.setup();
    const kept = renderForm(httpEnvelope);
    await save(user);
    expect(kept.onSave.mock.calls[0][0].fields).toEqual({});
    kept.unmount();

    const removed = renderForm(httpEnvelope);
    await user.click(screen.getByRole("combobox", { name: "Bearer 凭据值类型" }));
    await user.click(screen.getByRole("option", { name: "移除" }));
    await save(user);
    expect(removed.onSave.mock.calls[0][0].fields.bearer).toEqual({ action: "delete" });
    removed.unmount();

    const replaced = renderForm(httpEnvelope);
    await user.click(screen.getByRole("combobox", { name: "Bearer 凭据值类型" }));
    await user.click(screen.getByRole("option", { name: "新凭据" }));
    await user.type(screen.getByLabelText("Bearer 凭据值"), "fresh-token");
    await save(user);
    expect(replaced.onSave.mock.calls[0][0].fields.bearer).toEqual({
      action: "replace",
      value: { mode: "secretRef", reference: "secret-new" },
    });
  });

  it("switches the transport only through a complete re-definition", async () => {
    const user = userEvent.setup();
    const { onSave } = renderForm(stdioEnvelope);

    await user.click(screen.getByRole("combobox", { name: "传输方式" }));
    await user.click(screen.getByRole("option", { name: "HTTP（远程端点）" }));
    expect(screen.getByText(/切换传输不会携带旧传输的任何字段/)).toBeInTheDocument();
    expect(screen.queryByLabelText("启动命令")).not.toBeInTheDocument();

    // Submit explicitly: after a Radix Select interaction jsdom does not run
    // the submit button's implicit activation (browsers always do).
    const form = screen.getByRole("form", { name: "编辑 MCP 服务" });
    fireEvent.submit(form);
    expect(screen.getByRole("alert").textContent).toContain("服务地址不能为空");
    expect(onSave).not.toHaveBeenCalled();

    await user.type(screen.getByLabelText("服务地址"), "https://mcp.example.test/v2");
    fireEvent.submit(form);

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    const request = onSave.mock.calls[0][0];
    expect(request.transport).toEqual({
      transport: "http",
      url: "https://mcp.example.test/v2",
      headers: {},
    });
    expect(request.fields).toEqual({});
  });

  it("cancels without saving", async () => {
    const user = userEvent.setup();
    const { onSave, onCancel } = renderForm(stdioEnvelope);

    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(onSave).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
