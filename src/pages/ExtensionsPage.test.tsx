import { expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  listExtensionsMock,
  prepareExtensionPlanMock,
  applyExtensionPlanMock,
  recoverExtensionTransactionsMock,
  historyRecord,
  workspace,
  planView,
  renderPage,
  openMcpDetail,
  sensitivePlanView,
} from "../test/extensions-page";

it("renders the type tabs, the row list with per-client toggles, and the history", async () => {
  renderPage();

  expect(await screen.findByRole("tab", { name: "Skills" })).toHaveAttribute("aria-selected", "true");
  for (const [name, panelId] of [
    ["Skills", "ext-workspace-skill-panel"],
    ["MCP", "ext-workspace-mcp-panel"],
    ["全局指令", "ext-workspace-instructions-panel"],
  ]) {
    const tab = screen.getByRole("tab", { name });
    const panel = document.getElementById(panelId)!;
    expect(tab).toHaveAttribute("aria-controls", panelId);
    expect(panel).toHaveAttribute("role", "tabpanel");
    expect(panel).toHaveAttribute("aria-labelledby", tab.id);
  }
  expect(document.getElementById("ext-workspace-mcp-panel")!).toHaveAttribute("hidden");
  const list = await screen.findByRole("list", { name: "扩展列表" });
  expect(within(list).getByText("接口规范")).toBeInTheDocument();
  expect(within(list).getByRole("button", { name: "Codex：已一致" })).toHaveAttribute("aria-pressed", "true");
  expect(within(list).getByRole("button", { name: "Claude：未部署" })).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  expect(within(list).queryByText("docs")).not.toBeInTheDocument();
  expect(screen.getByText("共 1 项")).toBeInTheDocument();
  // The cc-switch count bar: library total on the left, per-client count
  // chips (the bulk deploy toggles) on the right (2026-09-12 用户指令).
  expect(screen.getByText("Skills · 1")).toBeInTheDocument();
  expect(
    screen.getByRole("checkbox", { name: "停用全部扩展的 Codex 部署（当前 1 项）" }),
  ).toHaveAttribute("aria-checked", "true");
  expect(
    screen.getByRole("checkbox", { name: "启用全部扩展的 Claude 部署（当前 0 项）" }),
  ).toHaveAttribute("aria-checked", "false");

  const user = userEvent.setup();
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  const mcpList = await screen.findByRole("list", { name: "扩展列表" });
  expect(within(mcpList).getByText("docs")).toBeInTheDocument();
  expect(within(mcpList).getByRole("button", { name: "Codex：未部署" })).toBeInTheDocument();
  expect(screen.getByText("MCP · 1")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "操作历史" }));
  const history = screen.getByRole("region", { name: "操作历史" });
  expect(within(history).getByText("安装")).toBeInTheDocument();
  expect(within(history).getByText("Codex 用户配置：已应用")).toBeInTheDocument();
  expect(within(history).getByRole("button", { name: "恢复" })).toBeEnabled();
});

it("installs to a client from the row toggle in one click", async () => {
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("tab", { name: "MCP" }));
  await user.click(await screen.findByRole("button", { name: "Codex：未部署" }));

  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [
      {
        operation: "install",
        definitionId: "ext-mcp-1",
        targets: [{ scope: "app", client: "codex" }],
      },
    ],
  });
  // The toggle applies the plan immediately; no preview dialog appears.
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
});

it("opens a modal detail with facts, bindings, and capabilities", async () => {
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  expect(within(detail).getByText("npx")).toBeInTheDocument();
  expect(within(detail).getByText("已配置 2 个启动参数")).toBeInTheDocument();
  expect(within(detail).queryByText("-y docs-server")).not.toBeInTheDocument();
  expect(within(detail).getByText("DOCS_TOKEN")).toBeInTheDocument();
  expect(within(detail).getByText("已设置凭据")).toBeInTheDocument();
  expect(within(detail).getByText("尚未部署到任何客户端")).toBeInTheDocument();
  const capabilities = screen.getByRole("complementary", { name: "客户端能力" });
  expect(capabilities).toHaveTextContent("已核实 · 0.42.0");
  expect(capabilities).toHaveTextContent("未核实 · 需要隔离环境验证");

  await user.click(screen.getByRole("button", { name: "关闭窗口" }));
  expect(screen.queryByRole("region", { name: "扩展详情 docs" })).not.toBeInTheDocument();
});

it("applies a multi-target install from the detail in one click, confirming only a sensitive write", async () => {
  // The multi-target install writes sensitive connection data, so it is the
  // one write shape that stops for an explicit confirmation.
  prepareExtensionPlanMock.mockResolvedValue(sensitivePlanView);
  applyExtensionPlanMock.mockResolvedValue({
    record: historyRecord,
    rejected: null,
    rolledBack: false,
  });
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("checkbox", { name: "安装目标 Codex 用户配置" }));
  await user.click(within(detail).getByRole("checkbox", { name: "安装目标 Claude 用户配置" }));
  await user.click(within(detail).getByRole("button", { name: "安装到所选目标" }));

  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [
      {
        operation: "install",
        definitionId: "ext-mcp-1",
        targets: [
          { scope: "app", client: "codex" },
          { scope: "app", client: "claude" },
        ],
      },
    ],
  });
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();

  const sheet = await screen.findByRole("dialog", { name: "确认安装（写入敏感数据）" });
  expect(within(sheet).getByText("警告：示例警告：将新增服务条目")).toBeInTheDocument();
  expect(within(sheet).getByText(/连接地址、参数或凭据值/)).toBeInTheDocument();
  expect(within(sheet).getByText("mcp_servers.docs")).toBeInTheDocument();

  await user.click(within(sheet).getByRole("button", { name: "确认写入" }));

  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledTimes(1));
  expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-sensitive", true);
  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "确认安装（写入敏感数据）" })).not.toBeInTheDocument(),
  );
});

it("shows a kind-specific empty state and focused add and import entries", async () => {
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [], history: [] });
  const user = userEvent.setup();
  renderPage();
  expect(await screen.findByRole("heading", { name: "还没有 Skills" })).toBeInTheDocument();
  expect(screen.queryByRole("form")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "从本机发现" })).toBeEnabled();
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  expect(screen.getByRole("heading", { name: "还没有 MCP 服务" })).toBeInTheDocument();
  await user.click(screen.getAllByRole("button", { name: "添加 MCP" })[0]);
  expect(await screen.findByRole("dialog", { name: "添加 MCP" })).toBeInTheDocument();
});

it("lets the user run recovery for a pending extension transaction", async () => {
  listExtensionsMock.mockResolvedValue({ ...workspace, recoveryRequired: ["op-recover"] });
  recoverExtensionTransactionsMock.mockResolvedValue(["op-recover：客户端与扩展库均已恢复到操作前状态"]);
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("button", { name: "尝试恢复" }));

  await waitFor(() => expect(recoverExtensionTransactionsMock).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(listExtensionsMock).toHaveBeenCalledTimes(2));
});
