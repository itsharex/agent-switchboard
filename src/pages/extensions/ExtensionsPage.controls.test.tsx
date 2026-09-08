import { expect, it } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ExtensionListItem, ExtensionPlanView } from "../../api/client";
import {
  applyExtensionPlanMock,
  listExtensionsMock,
  mcpItem,
  planView,
  prepareExtensionPlanMock,
  renderPage,
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
  await user.click(screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 0 项）" }));
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [
      { operation: "install", definitionId: "unbound", targets: [{ scope: "app", client: "codex" }] },
      { operation: "enable", bindingId: "paused-binding" },
    ],
  });
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
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
  expect(screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 1 项）" })).toHaveAttribute(
    "aria-checked",
    "mixed",
  );
  await user.click(toggle);
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "enable", bindingId: "project-binding" }],
  });
});

it("a Claude project disable waits for an explicit scope before preparing the whole batch", async () => {
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
  expect(screen.getByRole("button", { name: "生成停用预览" })).toBeDisabled();
  await user.click(screen.getByRole("combobox", { name: "Claude 项目 Skill 停用规则写入位置" }));
  await user.click(screen.getByRole("option", { name: "个人本地设置" }));
  await user.click(screen.getByRole("button", { name: "生成停用预览" }));
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "disable", bindingId: "bind-1", sharedSettings: false }],
  });
});

it("disables a bulk control when no item supports its client", async () => {
  listExtensionsMock.mockResolvedValue({
    ...workspace,
    items: [{ ...skillItem, hostScoped: "claude", bindings: [] }],
  });
  renderPage();
  await screen.findByRole("button", { name: "管理 接口规范" });
  expect(screen.getByRole("checkbox", { name: "启用全部扩展的 Codex 部署（当前 0 项）" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Codex：不支持当前类型" })).toBeDisabled();
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

it("closes only the top modal with Escape and restores focus to the list afterwards", async () => {
  prepareExtensionPlanMock.mockResolvedValue(planView);
  const user = userEvent.setup();
  renderPage();
  const trigger = await screen.findByRole("button", { name: "管理 接口规范" });
  await user.click(trigger);
  await user.click(screen.getByRole("checkbox", { name: "安装目标 Claude 用户配置" }));
  await user.click(screen.getByRole("button", { name: "安装到所选目标" }));
  expect(await screen.findByRole("dialog", { name: "安装预览" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog", { name: "安装预览" })).not.toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "扩展详情 接口规范" })).toBeInTheDocument();
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
  const user = userEvent.setup();
  renderPage();
  const toggle = await screen.findByRole("button", { name: "Claude：未部署" });
  fireEvent.click(toggle);
  fireEvent.click(toggle);
  expect(prepareExtensionPlanMock).toHaveBeenCalledTimes(1);
  resolvePlan(planView);
  await screen.findByRole("dialog", { name: "安装预览" });
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
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

it("guides bound deletion to installation management without offering an invalid confirmation", async () => {
  const user = userEvent.setup();
  renderPage();
  await user.click(await screen.findByRole("button", { name: "删除 接口规范" }));
  const dialog = screen.getByRole("dialog", { name: "删除扩展定义" });
  expect(within(dialog).queryByRole("button", { name: "确认删除" })).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "管理安装" }));
  expect(await screen.findByRole("dialog", { name: "扩展详情 接口规范" })).toBeInTheDocument();
});

it("Escape dismisses a dropdown before its containing editor", async () => {
  const user = userEvent.setup();
  renderPage();
  await user.click(await screen.findByRole("tab", { name: "MCP" }));
  await user.click(screen.getByRole("button", { name: "添加 MCP" }));
  await user.click(screen.getByRole("combobox", { name: "传输方式" }));
  expect(screen.getByRole("option", { name: "HTTP（远程端点）" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "添加 MCP" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
