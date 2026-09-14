import { expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ExtensionListItem } from "../../api/client";
import {
  listExtensionsMock,
  prepareExtensionPlanMock,
  applyExtensionPlanMock,
  checkSkillUpdatesMock,
  updateSkillDefinitionMock,
  setBindingLockMock,
  skillItem,
  mcpItem,
  workspace,
  planView,
  renderPage,
} from "../../test/extensions-page";

it("updates a Skill into the library and deploys the enabled bindings immediately", async () => {
  checkSkillUpdatesMock.mockResolvedValue([
    {
      definitionId: "ext-skill-1",
      upToDate: false,
      currentCommit: "a".repeat(40),
      newCommit: "b".repeat(40),
      newDigest: "b".repeat(64),
      changedFiles: [
        { relativePath: "SKILL.md", action: "modified" },
        { relativePath: "references/style.md", action: "added" },
      ],
      error: null,
    },
  ]);
  updateSkillDefinitionMock.mockResolvedValue({
    id: "ext-skill-1",
    name: "接口规范",
    revision: 2,
  });
  prepareExtensionPlanMock.mockResolvedValue({
    ...planView,
    planId: "plan-skill-update",
    operations: [
      {
        ...planView.operations[0],
        definitionId: "ext-skill-1",
        definitionRevision: 2,
        operation: "update",
        targets: [
          {
            target: { scope: "app", client: "codex" },
            warnings: [],
            changes: [],
            files: [{ relativePath: "SKILL.md", action: "deploy" }],
            writesSensitiveConnectionData: false,
          },
        ],
      },
    ],
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("button", { name: "管理 接口规范" }));
  const detail = await screen.findByRole("region", { name: "扩展详情 接口规范" });
  await user.click(within(detail).getByRole("button", { name: "检查更新" }));
  expect(await within(detail).findByText("修改 SKILL.md")).toBeInTheDocument();
  expect(within(detail).getByText("新增 references/style.md")).toBeInTheDocument();
  await user.click(within(detail).getByRole("button", { name: "更新到新内容" }));

  await waitFor(() => expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-1", "b".repeat(64)));
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "ext-skill-1" }],
  });
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-skill-update", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
});

it("updates the whole library and deploys only the Skills with active deployments", async () => {
  const secondSkill: ExtensionListItem = {
    ...skillItem,
    id: "ext-skill-2",
    name: "文档检索",
    bindings: [],
    dependencyStates: [],
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [skillItem, secondSkill, mcpItem] });
  checkSkillUpdatesMock.mockResolvedValue([
    {
      definitionId: "ext-skill-1",
      upToDate: false,
      currentCommit: "a".repeat(40),
      newCommit: "b".repeat(40),
      newDigest: "b".repeat(64),
      changedFiles: [{ relativePath: "SKILL.md", action: "modified" }],
      error: null,
    },
    {
      definitionId: "ext-skill-2",
      upToDate: false,
      currentCommit: "c".repeat(40),
      newCommit: "d".repeat(40),
      newDigest: "d".repeat(64),
      changedFiles: [],
      error: null,
    },
  ]);
  updateSkillDefinitionMock.mockResolvedValue({
    id: "ext-skill-1",
    name: "接口规范",
    revision: 2,
  });
  prepareExtensionPlanMock.mockResolvedValue({
    ...planView,
    planId: "plan-batch",
    operations: [],
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(screen.getByRole("button", { name: "检查更新" }));

  expect(checkSkillUpdatesMock).toHaveBeenCalledWith(["ext-skill-1", "ext-skill-2"]);
  expect(await screen.findAllByText("可更新")).toHaveLength(2);

  // Update-all lives on the count bar, next to the deploy count chips.
  await user.click(screen.getByRole("button", { name: "全部更新（2）" }));

  await waitFor(() => expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-1", "b".repeat(64)));
  await waitFor(() => expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-2", "d".repeat(64)));
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "ext-skill-1" }],
  });
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-batch", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
  expect(screen.queryByText("可更新")).not.toBeInTheDocument();
});

it("updates one Skill from its row action and deploys it immediately", async () => {
  const secondSkill: ExtensionListItem = {
    ...skillItem,
    id: "ext-skill-2",
    name: "文档检索",
    bindings: [],
    dependencyStates: [],
  };
  listExtensionsMock.mockResolvedValue({ ...workspace, items: [skillItem, secondSkill, mcpItem] });
  checkSkillUpdatesMock.mockResolvedValue([
    {
      definitionId: "ext-skill-1",
      upToDate: false,
      currentCommit: "a".repeat(40),
      newCommit: "b".repeat(40),
      newDigest: "b".repeat(64),
      changedFiles: [],
      error: null,
    },
    {
      definitionId: "ext-skill-2",
      upToDate: true,
      currentCommit: "c".repeat(40),
      newCommit: null,
      newDigest: null,
      changedFiles: [],
      error: null,
    },
  ]);
  updateSkillDefinitionMock.mockResolvedValue({
    id: "ext-skill-1",
    name: "接口规范",
    revision: 2,
  });
  prepareExtensionPlanMock.mockResolvedValue({
    ...planView,
    planId: "plan-row-update",
    operations: [],
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(screen.getByRole("button", { name: "检查更新" }));
  // Only the outdated row carries the badge; the count bar carries update-all.
  expect(await screen.findByText("可更新")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "全部更新（1）" })).toBeEnabled();

  await user.click(screen.getByRole("button", { name: "更新 接口规范" }));

  await waitFor(() => expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-1", "b".repeat(64)));
  expect(updateSkillDefinitionMock).not.toHaveBeenCalledWith("ext-skill-2", expect.anything());
  expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "ext-skill-1" }],
  });
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-row-update", true));
  expect(screen.queryByRole("dialog", { name: /预览/ })).not.toBeInTheDocument();
});

it("pins and unpins a Skill binding's version from the detail view", async () => {
  setBindingLockMock.mockResolvedValue(undefined);
  const lockedWorkspace = {
    ...workspace,
    items: [
      {
        ...skillItem,
        bindings: [
          {
            ...skillItem.bindings[0],
            lockedDigest: "a".repeat(64),
          },
        ],
      },
      mcpItem,
    ],
  };
  listExtensionsMock.mockReset();
  listExtensionsMock.mockResolvedValueOnce(workspace).mockResolvedValueOnce(lockedWorkspace);
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("button", { name: "管理 接口规范" }));
  const detail = await screen.findByRole("region", { name: "扩展详情 接口规范" });
  await user.click(within(detail).getByRole("button", { name: /^固定版本/ }));

  await waitFor(() => expect(setBindingLockMock).toHaveBeenCalledWith("bind-1", true));
  // The refreshed workspace carries the pinned digest; the row now shows
  // the pinned pill and offers the release action.
  expect(await screen.findByText(`已固定 ${"a".repeat(12)}`)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /^解除固定/ })).toBeEnabled();
});
