import { expect, it } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ExtensionListItem, ExtensionPlanView } from "../../api/client";
import {
  applyExtensionPlanMock,
  deleteExtensionMock,
  listExtensionsMock,
  mcpItem,
  planView,
  prepareExtensionPlanMock,
  renderPage,
  sensitivePlanView,
  skillItem,
  workspace,
} from "../../test/extensions-page";

it("bulk enable includes unbound and paused items across search results, excluding unsupported clients", async () => {
  const unbound: ExtensionListItem = { ...skillItem, id: "unbound", name: "新技能", bindings: [] };
  const paused: ExtensionListItem = {
    ...skillItem,
    id: "paused",
    name: "已停用技能",
    bindings: [{ ...skillItem.bindings[0], id: "paused-binding", resourceId: "paused", desired: "disabled" }],
  };
  const unsupported: ExtensionListItem = {
    ...skillItem,
    id: "claude-only",
    hostScoped: "claude",
    bindings: [],
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [unbound, paused, unsupported] });
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: "管理 新技能" });
  await user.type(screen.getByRole("searchbox", { name: "搜索扩展" }), "新技能");
  expect(screen.queryByRole("button", { name: "管理 已停用技能" })).not.toBeInTheDocument();
  // The count chip is the library-wide bulk deploy toggle.
  await user.click(
    screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 0 项）" }),
  );
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [
      { operation: "install", definitionId: "unbound", targets: [{ scope: "app", client: "codex" }] },
      { operation: "enable", bindingId: "paused-binding" },
    ],
  });
  // One action toggles the whole library: the plan applies immediately.
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
});

it("mixed scope switches enable only the paused bindings without reinstalling an existing scope", async () => {
  const mixed: ExtensionListItem = {
    ...skillItem,
    bindings: [
      skillItem.bindings[0],
      {
        ...skillItem.bindings[0],
        id: "project-binding",
        desired: "disabled",
        target: { scope: "projectShared", client: "codex", projectId: "project" },
      },
    ],
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [mixed] });
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();
  const toggle = await screen.findByRole("button", { name: "Codex：已一致 · 部分启用" });
  expect(toggle).toHaveAttribute("aria-pressed", "mixed");
  await user.click(
    screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 1 项）" }),
  );
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "enable", bindingId: "project-binding" }],
  });
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
});

it("a Claude project disable waits for an explicit scope, then applies immediately", async () => {
  const projectSkill: ExtensionListItem = {
    ...skillItem,
    bindings: [
      {
        ...skillItem.bindings[0],
        target: { scope: "projectShared", client: "claude", projectId: "project" },
      },
    ],
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [projectSkill] });
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();
  await user.click(await screen.findByRole("button", { name: "Claude：已一致" }));
  expect(prepareExtensionPlanMock).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "执行停用" })).toBeDisabled();
  await user.click(screen.getByRole("combobox", { name: "Claude 项目 Skill 停用规则写入位置" }));
  await user.click(screen.getByRole("option", { name: "个人本地设置" }));
  await user.click(screen.getByRole("button", { name: "执行停用" }));
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "disable", bindingId: "bind-1", sharedSettings: false }],
  });
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
});

it("disables a bulk action when no item supports its client", async () => {
  listExtensionsMock.mockResolvedValue({
    ...workspace,
    items: [{ ...skillItem, hostScoped: "claude", bindings: [] }],
  });
  renderPage();
  await screen.findByRole("button", { name: "管理 接口规范" });
  expect(screen.getByRole("button", { name: "Codex：不支持当前类型" })).toBeDisabled();
  expect(
    screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 0 项）" }),
  ).toBeDisabled();
});

it("keeps initial load failures distinct from an empty library and supports retry", async () => {
  listExtensionsMock.mockRejectedValueOnce({ code: "read", message: "读取失败" });
  const user = userEvent.setup();
  renderPage();
  expect(await screen.findByRole("heading", { name: "扩展库加载失败" })).toBeInTheDocument();
  expect(screen.queryByText("还没有 Skills")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新加载" }));
  expect(await screen.findByRole("button", { name: "管理 接口规范" })).toBeInTheDocument();
});

it("still confirms a sensitive connection write and closes only the top modal with Escape", async () => {
  prepareExtensionPlanMock.mockResolvedValue(sensitivePlanView);
  const user = userEvent.setup();
  renderPage();
  const trigger = await screen.findByRole("button", { name: "管理 接口规范" });
  await user.click(trigger);
  await user.click(screen.getByRole("checkbox", { name: "安装目标 Claude 用户配置" }));
  await user.click(screen.getByRole("button", { name: "安装到所选目标" }));
  expect(await screen.findByRole("dialog", { name: "确认安装（写入敏感数据）" })).toBeInTheDocument();
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  // Nothing is written until the sensitive write is confirmed.
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog", { name: "确认安装（写入敏感数据）" })).not.toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "扩展详情 接口规范" })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole("button", { name: "关闭窗口" })).toBeEnabled());
  await user.keyboard("{Escape}");
  await waitFor(() => expect(trigger).toHaveFocus());
});

it("serializes repeated toggle clicks before the busy prop has re-rendered", async () => {
  let resolvePlan!: (view: ExtensionPlanView) => void;
  prepareExtensionPlanMock.mockImplementation(
    () =>
      new Promise((resolve) => {
        resolvePlan = resolve;
      }),
  );
  renderPage();
  const toggle = await screen.findByRole("button", { name: "Claude：未部署" });
  fireEvent.click(toggle);
  fireEvent.click(toggle);
  expect(prepareExtensionPlanMock).toHaveBeenCalledTimes(1);
  resolvePlan(planView);
  // The resolved plan applies inside the same exclusive run — no dialog.
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
});

it("searches MCP command text without searching credential slots", async () => {
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [mcpItem] });
  const user = userEvent.setup();
  renderPage();
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索扩展" }), "npx");
  expect(await screen.findByRole("button", { name: "管理 docs" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "清除搜索" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索扩展" }), "DOCS_TOKEN");
  expect(screen.queryByRole("button", { name: "管理 docs" })).not.toBeInTheDocument();
});

it("deletes a bound definition in one confirmation, revoking its deployments first", async () => {
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();
  await user.click(await screen.findByRole("button", { name: "删除 接口规范" }));
  const dialog = screen.getByRole("dialog", { name: "删除扩展定义" });
  expect(within(dialog).getByText(/仍有 1 个客户端安装/)).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "删除并撤销部署" }));
  // The binding removal goes through the shared immediate-apply pipeline,
  // then the now-unbound definition is deleted.
  await waitFor(() =>
    expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
      operations: [{ operation: "remove", bindingId: "bind-1" }],
    }),
  );
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true));
  await waitFor(() => expect(deleteExtensionMock).toHaveBeenCalledWith("ext-skill-1", true));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "删除扩展定义" })).not.toBeInTheDocument());
});

it("Escape dismisses a dropdown before its containing editor", async () => {
  const user = userEvent.setup();
  renderPage();
  await user.click(await screen.findByRole("tab", { name: "MCP" }));
  await user.click(screen.getByRole("button", { name: "添加 MCP" }));
  await user.click(screen.getByRole("combobox", { name: "MCP 常用预设" }));
  expect(screen.getByRole("option", { name: /fetch/ })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "添加 MCP" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
