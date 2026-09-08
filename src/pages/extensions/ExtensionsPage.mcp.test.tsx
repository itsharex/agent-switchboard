import { expect, it } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ExtensionListItem, type McpEditViewEnvelope } from "../../api/client";
import {
  listExtensionsMock,
  prepareExtensionPlanMock,
  applyExtensionPlanMock,
  checkMcpConnectionMock,
  getMcpCheckMock,
  getMcpEditViewMock,
  updateMcpDefinitionMock,
  skillItem,
  mcpItem,
  workspace,
  planView,
  checkResult,
  renderPage,
  openMcpDetail,
} from "../../test/extensions-page";

it("shows the bounded check stages and polls until the result lands", async () => {
  checkMcpConnectionMock.mockResolvedValue({ checkId: "check-1" });
  let polls = 0;
  getMcpCheckMock.mockImplementation(async () => {
    polls += 1;
    return polls >= 2 ? checkResult : null;
  });
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("checkbox", { name: "安装目标 Codex 用户配置" }));
  await user.click(within(detail).getByRole("button", { name: "连接检测" }));

  expect(checkMcpConnectionMock).toHaveBeenCalledWith("ext-mcp-1", { scope: "app", client: "codex" }, true);
  const stages = await screen.findByRole("list", { name: "检测阶段" });
  expect(within(stages).getByText("连接")).toBeInTheDocument();
  expect(within(stages).getByText("initialize")).toBeInTheDocument();
  expect(within(stages).getByText("读取目录")).toBeInTheDocument();
  expect(within(stages).getByText("完成")).toBeInTheDocument();

  expect(
    await screen.findByText(/工具 3 · 资源 1 · 提示词 2/, undefined, { timeout: 3000 }),
  ).toBeInTheDocument();
  expect(screen.getByText("通过")).toBeInTheDocument();
  expect(getMcpCheckMock).toHaveBeenCalledWith("check-1");
});

it("edits an existing MCP through the redacted view and requires the deployment preview", async () => {
  const deployedMcp: ExtensionListItem = {
    ...mcpItem,
    bindings: [
      {
        schemaVersion: 2,
        id: "bind-mcp-1",
        resourceId: "ext-mcp-1",
        target: { scope: "app", client: "codex" },
        desired: "enabled",
        updatedAt: "2026-09-01T08:00:00Z",
        fileState: "inSync",
        warnings: [],
      },
    ],
  };
  const deployedWorkspace = { ...workspace, items: [skillItem, deployedMcp] };
  const editView: McpEditViewEnvelope = {
    id: "ext-mcp-1",
    revision: 1,
    name: "docs",
    transport: "stdio",
    command: "npx",
    args: ["-y", "docs-server"],
    env: [{ name: "DOCS_TOKEN", value: { mode: "secretConfigured" } }],
    codexOptions: null,
  };
  listExtensionsMock.mockResolvedValue(deployedWorkspace);
  getMcpEditViewMock.mockResolvedValue(editView);
  updateMcpDefinitionMock.mockResolvedValue({
    id: "ext-mcp-1",
    name: "docs",
    revision: 2,
  });
  prepareExtensionPlanMock.mockResolvedValue({
    ...planView,
    planId: "plan-edit",
    operations: [
      {
        ...planView.operations[0],
        definitionRevision: 2,
        operation: "update",
      },
    ],
  });
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("button", { name: "编辑定义" }));

  const form = await screen.findByRole("form", { name: "编辑 MCP 服务" });
  expect(getMcpEditViewMock).toHaveBeenCalledWith("ext-mcp-1");
  // The editor prefills editable material and marks the stored credential.
  expect(within(form).getByLabelText("启动命令")).toHaveValue("npx");
  expect(within(form).getByText("已设置凭据（保持不变）")).toBeInTheDocument();
  expect(within(form).queryByRole("region", { name: "扩展详情 docs" })).not.toBeInTheDocument();

  const command = within(form).getByLabelText("启动命令");
  await user.clear(command);
  await user.type(command, "docker");
  await user.click(within(form).getByRole("button", { name: "保存修改" }));

  await waitFor(() =>
    expect(updateMcpDefinitionMock).toHaveBeenCalledWith("ext-mcp-1", {
      expectedRevision: 1,
      // Only the touched command is named; the stored credential stays
      // unnamed and survives inside the backend.
      fields: { command: { action: "replace", value: "docker" } },
    }),
  );
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "ext-mcp-1" }],
  });
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
  expect(await screen.findByRole("dialog", { name: "更新预览" })).toBeInTheDocument();
  // The form closed back to the refreshed detail after the save.
  await waitFor(() => expect(screen.queryByRole("form", { name: "编辑 MCP 服务" })).not.toBeInTheDocument());
});

it("edits an existing MCP without bindings and skips the deployment preview", async () => {
  const editView: McpEditViewEnvelope = {
    id: "ext-mcp-1",
    revision: 1,
    name: "docs",
    transport: "http",
    url: "https://mcp.example.test/v1",
    headers: [],
    bearer: null,
  };
  getMcpEditViewMock.mockResolvedValue(editView);
  updateMcpDefinitionMock.mockResolvedValue({
    id: "ext-mcp-1",
    name: "docs",
    revision: 2,
  });
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("button", { name: "编辑定义" }));
  const form = await screen.findByRole("form", { name: "编辑 MCP 服务" });

  // After a Radix Select-free interaction the implicit submit works; use
  // the button directly for the http form.
  const url = within(form).getByLabelText("服务地址");
  await user.clear(url);
  await user.type(url, "https://mcp.example.test/v2");
  fireEvent.submit(form);

  await waitFor(() =>
    expect(updateMcpDefinitionMock).toHaveBeenCalledWith("ext-mcp-1", {
      expectedRevision: 1,
      fields: { url: { action: "replace", value: "https://mcp.example.test/v2" } },
    }),
  );
  expect(prepareExtensionPlanMock).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.queryByRole("form", { name: "编辑 MCP 服务" })).not.toBeInTheDocument());
});

it("renames the server key and previews deployment even when every binding is disabled", async () => {
  const disabledMcp: ExtensionListItem = {
    ...mcpItem,
    bindings: [
      {
        schemaVersion: 2,
        id: "bind-mcp-1",
        resourceId: "ext-mcp-1",
        target: { scope: "app", client: "codex" },
        desired: "disabled",
        updatedAt: "2026-09-01T08:00:00Z",
        fileState: "inSync",
        warnings: [],
      },
    ],
  };
  const editView: McpEditViewEnvelope = {
    id: "ext-mcp-1",
    revision: 1,
    name: "docs",
    transport: "http",
    url: "https://mcp.example.test/v1",
    headers: [],
    bearer: null,
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [skillItem, disabledMcp] });
  getMcpEditViewMock.mockResolvedValue(editView);
  updateMcpDefinitionMock.mockResolvedValue({
    id: "ext-mcp-1",
    name: "docs_v2",
    revision: 2,
  });
  prepareExtensionPlanMock.mockResolvedValue({
    ...planView,
    planId: "plan-rename",
    operations: [
      {
        ...planView.operations[0],
        definitionRevision: 2,
        operation: "update",
      },
    ],
  });
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("button", { name: "编辑定义" }));
  const form = await screen.findByRole("form", { name: "编辑 MCP 服务" });

  const name = within(form).getByLabelText(/服务名称/);
  await user.clear(name);
  await user.type(name, "docs_v2");
  fireEvent.submit(form);

  await waitFor(() =>
    expect(updateMcpDefinitionMock).toHaveBeenCalledWith("ext-mcp-1", {
      expectedRevision: 1,
      // Only the key rename travels; no field-level patch is needed.
      serverKey: "docs_v2",
      fields: {},
    }),
  );
  // A disabled binding still moves its native key inside the same plan,
  // so the deployment preview is mandatory here too.
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "ext-mcp-1" }],
  });
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
  expect(await screen.findByRole("dialog", { name: "更新预览" })).toBeInTheDocument();
});
