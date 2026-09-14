import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { ExtensionListItem, McpEditViewEnvelope, SkillEditorView } from "../../api/client";
import { ExtensionWorkspaceDialogs } from "./ExtensionWorkspaceDialogs";
import { api, mcp, notify, prepared, skill, useTestExtensionWorkspace, workspace } from "./extension-apply.test-support";

function EditorHarness({ item }: { item: ExtensionListItem }) {
  const w = useTestExtensionWorkspace();
  return <>
    <button disabled={!w.ext.loaded} onClick={() => void w.edit(item)}>Open editor</button>
    <ExtensionWorkspaceDialogs workspace={w} />
  </>;
}

async function openMcpEditor() {
  const deployed = { ...mcp, bindings: [{ ...skill.bindings[0], resourceId: "mcp" }] };
  api.listExtensions.mockResolvedValue({ ...workspace, items: [deployed] });
  const envelope: McpEditViewEnvelope = { id: "mcp", revision: 1, name: "docs", transport: "stdio",
    command: "npx", args: [], env: [], codexOptions: null };
  api.getMcpEditView.mockResolvedValue(envelope);
  api.updateMcpDefinition.mockImplementation(async () => {
    api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...deployed, revision: 2 }] });
    return { id: "mcp", name: "docs", revision: 2 };
  });
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const user = userEvent.setup();
  render(<EditorHarness item={deployed} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Open editor" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Open editor" }));
  let form = await screen.findByRole("form", { name: "编辑 MCP 服务" });
  await user.click(screen.getByRole("button", { name: "配置向导" }));
  const wizard = await screen.findByRole("form", { name: "MCP 配置向导" });
  fireEvent.change(within(wizard).getByLabelText("启动命令"), { target: { value: "docker" } });
  await user.click(within(wizard).getByRole("button", { name: "应用配置" }));
  form = await screen.findByRole("form", { name: "编辑 MCP 服务" });
  fireEvent.submit(form);
  await waitFor(() => expect(api.updateMcpDefinition).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(api.prepareExtensionPlan).toHaveBeenCalledTimes(1));
  await screen.findByRole("button", { name: "确认写入" });
  return user;
}

it("keeps sensitive confirmation actionable while the MCP editor holds the write lock", async () => {
  const user = await openMcpEditor();
  expect(screen.getByText("编辑 MCP · docs")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "确认写入" })).toBeEnabled();
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "确认写入" }));
  await waitFor(() => expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1));
  expect(await screen.findByRole("region", { name: "扩展详情 docs" })).toBeInTheDocument();
  expect(screen.queryByRole("form", { name: "编辑 MCP 服务" })).not.toBeInTheDocument();
});

it("cancels sensitive deployment and returns to the saved definition without a success message", async () => {
  const user = await openMcpEditor();
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(await screen.findByRole("region", { name: "扩展详情 docs" })).toBeInTheDocument();
  expect(api.updateMcpDefinition).toHaveBeenCalledTimes(1);
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("reloads the saved Skill revision while its deployment confirmation is still pending", async () => {
  const initial: SkillEditorView = { id: "skill", revision: 1, contentDigest: skill.contentDigest,
    manifest: skill.manifest, editable: true, files: [{ relativePath: "SKILL.md", text: "old", size: 3 }] };
  const saved = { ...initial, revision: 2, contentDigest: "b".repeat(64),
    files: [{ relativePath: "SKILL.md", text: "saved content", size: 13 }] };
  api.getSkillEditor.mockResolvedValueOnce(initial).mockResolvedValue(saved);
  api.updateSkillFiles.mockImplementation(async () => {
    api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...skill,
      revision: 2, contentDigest: saved.contentDigest,
    }] });
    return { id: "skill", name: "rules", revision: 2 };
  });
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const user = userEvent.setup();
  render(<EditorHarness item={skill} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Open editor" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Open editor" }));
  await screen.findByRole("textbox", { name: "编辑 SKILL.md" });
  await user.click(screen.getByRole("button", { name: "保存为新版本" }));
  await screen.findByRole("button", { name: "确认写入" });
  await waitFor(() => expect(api.getSkillEditor).toHaveBeenCalledTimes(2));
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(await screen.findByRole("textbox", { name: "编辑 SKILL.md" })).toHaveValue("saved content");
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});
