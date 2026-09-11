import { useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AppKind, GlobalPromptDocument } from "../api/client";
import {
  applyExtensionPlanMock,
  discoverExtensionsMock,
  getGlobalPromptDocumentMock,
  listExtensionsMock,
  prepareExtensionPlanMock,
  recoverExtensionTransactionsMock,
  renderPage,
  saveGlobalPromptDocumentMock,
  workspace,
} from "../test/extensions-page";
import { ExtensionsWorkspace } from "./ExtensionsWorkspace";
import type { ExtensionSection } from "./navigation";
import { usePromptDocuments } from "./usePromptDocuments";

const documents: Record<AppKind, GlobalPromptDocument> = {
  codex: { app: "codex", fileName: "AGENTS.md", content: "Codex 指令", contentHash: "codex-hash", exists: true },
  claude: { app: "claude", fileName: "CLAUDE.md", content: "Claude 指令", contentHash: "claude-hash", exists: true },
};
const reportError = vi.fn();
const clearError = vi.fn();

function WorkspaceHarness() {
  const [visible, setVisible] = useState(true);
  const [extensionSection, setExtensionSection] = useState<ExtensionSection>("skill");
  const [appFilter, selectApp] = useState<AppKind>("codex");
  const [busy, setBusy] = useState(false);
  const promptDocuments = usePromptDocuments({
    active: visible && extensionSection === "instructions", busy, setBusy, onError: reportError, clearError,
  });
  return (
    <>
      <button onClick={() => setVisible(!visible)}>{visible ? "离开扩展" : "返回扩展"}</button>
      {visible && <ExtensionsWorkspace model={{ extensionSection, setExtensionSection, appFilter,
        providers: { selectApp }, promptDocuments, busy, setBusy, reportError, clearError }} />}
    </>
  );
}

beforeEach(() => {
  reportError.mockReset();
  clearError.mockReset();
  discoverExtensionsMock.mockReset();
  getGlobalPromptDocumentMock.mockImplementation(async (app) => documents[app]);
  saveGlobalPromptDocumentMock.mockImplementation(async (app, content) => ({
    ...documents[app], content, contentHash: `${app}-saved-hash`,
  }));
});

it("edits and saves each real global document through its independent prompt callbacks", async () => {
  const user = userEvent.setup();
  render(<WorkspaceHarness />);
  await screen.findByRole("button", { name: "管理 接口规范" });
  expect(getGlobalPromptDocumentMock).not.toHaveBeenCalled();
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  const codex = await screen.findByRole("textbox", { name: "AGENTS.md 内容" });
  expect(screen.getByRole("radio", { name: "Codex" })).toBeChecked();
  expect(codex).toHaveValue(documents.codex.content);
  expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("list", { name: "扩展列表" })).not.toBeInTheDocument();
  expect(screen.queryByText("共 1 项")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "从本机发现" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "更多扩展操作" })).not.toBeInTheDocument();
  await user.clear(codex);
  await user.type(codex, "新的 Codex 指令");
  await user.click(screen.getByRole("button", { name: "保存 AGENTS.md" }));
  await waitFor(() => expect(saveGlobalPromptDocumentMock).toHaveBeenCalledWith(
    "codex", "新的 Codex 指令", "codex-hash", true,
  ));
  await user.click(screen.getByRole("radio", { name: "Claude" }));
  const claude = await screen.findByRole("textbox", { name: "CLAUDE.md 内容" });
  expect(claude).toHaveValue(documents.claude.content);
  await user.clear(claude);
  await user.type(claude, "新的 Claude 指令");
  await user.click(screen.getByRole("button", { name: "保存 CLAUDE.md" }));
  await waitFor(() => expect(saveGlobalPromptDocumentMock).toHaveBeenCalledWith(
    "claude", "新的 Claude 指令", "claude-hash", true,
  ));
  expect(saveGlobalPromptDocumentMock).toHaveBeenCalledTimes(2);
  expect(prepareExtensionPlanMock).not.toHaveBeenCalled();
  expect(applyExtensionPlanMock).not.toHaveBeenCalled();
  expect(discoverExtensionsMock).not.toHaveBeenCalled();
});

it("retains each client's draft and the selected instruction section after leaving the workspace", async () => {
  const user = userEvent.setup();
  render(<WorkspaceHarness />);
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  await user.type(await screen.findByRole("textbox", { name: "AGENTS.md 内容" }), " 草稿一");
  await user.click(screen.getByRole("radio", { name: "Claude" }));
  await user.type(screen.getByRole("textbox", { name: "CLAUDE.md 内容" }), " 草稿二");
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  expect(await screen.findByRole("button", { name: "管理 docs" })).toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  expect(screen.getByRole("textbox", { name: "CLAUDE.md 内容" })).toHaveValue("Claude 指令 草稿二");
  await user.click(screen.getByRole("button", { name: "离开扩展" }));
  await user.click(screen.getByRole("button", { name: "返回扩展" }));
  expect(screen.getByRole("tab", { name: "全局指令" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("textbox", { name: "CLAUDE.md 内容" })).toHaveValue("Claude 指令 草稿二");
  await user.click(screen.getByRole("radio", { name: "Codex" }));
  expect(screen.getByRole("textbox", { name: "AGENTS.md 内容" })).toHaveValue("Codex 指令 草稿一");
  await user.click(screen.getByRole("button", { name: "放弃草稿" }));
  expect(screen.getByRole("textbox", { name: "AGENTS.md 内容" })).toHaveValue(documents.codex.content);
  expect(getGlobalPromptDocumentMock).toHaveBeenCalledTimes(2);
  expect(saveGlobalPromptDocumentMock).not.toHaveBeenCalled();
});

it("preserves resource filters independently of the global instruction client", async () => {
  const user = userEvent.setup();
  render(<WorkspaceHarness />);
  await screen.findByRole("button", { name: "管理 接口规范" });
  await user.type(screen.getByRole("searchbox", { name: "搜索扩展" }), "接口");
  // The library's client filter is the shared segment radio group, not a
  // tablist: filtering is not panel switching (DESIGN.md §8).
  await user.click(screen.getByRole("radio", { name: "Codex" }));
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  await screen.findByRole("textbox", { name: "AGENTS.md 内容" });
  await user.click(screen.getByRole("radio", { name: "Claude" }));
  await user.click(screen.getByRole("tab", { name: "Skills" }));
  expect(screen.getByRole("searchbox", { name: "搜索扩展" })).toHaveValue("接口");
  expect(screen.getByRole("radio", { name: "Codex" })).toBeChecked();
  expect(screen.getByRole("button", { name: "管理 接口规范" })).toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  expect(screen.getByRole("radio", { name: "Claude" })).toBeChecked();
});

it("keeps extension recovery visible and actionable while editing instructions", async () => {
  listExtensionsMock.mockResolvedValue({ ...workspace, recoveryRequired: ["op-unresolved"] });
  recoverExtensionTransactionsMock.mockResolvedValue(["op-unresolved 已恢复"]);
  const user = userEvent.setup();
  render(<WorkspaceHarness />);
  await screen.findByRole("alert", { name: "扩展恢复告警" });
  await user.click(screen.getByRole("tab", { name: "全局指令" }));
  expect(screen.getByRole("alert", { name: "扩展恢复告警" })).toHaveTextContent("op-unresolved");
  await user.click(screen.getByRole("button", { name: "尝试恢复" }));
  await waitFor(() => expect(recoverExtensionTransactionsMock).toHaveBeenCalledTimes(1));
});

it("navigates all three sections with the keyboard and offers only resource types in discovery", async () => {
  const user = userEvent.setup();
  renderPage();
  await user.click(screen.getByRole("tab", { name: "Skills" }));
  await user.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "全局指令" })).toHaveFocus();
  expect(screen.getByRole("tabpanel", { name: "全局指令" })).toBeInTheDocument();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: "Skills" })).toHaveFocus();
  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("tab", { name: "全局指令" })).toHaveFocus();
  await user.keyboard("{Home}");
  expect(screen.getByRole("tab", { name: "Skills" })).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "从本机发现" }));
  const dialog = screen.getByRole("dialog", { name: "从本机发现" });
  const typeTabs = within(dialog).getByRole("tablist", { name: "扩展类型" });
  expect(within(typeTabs).getAllByRole("tab")).toHaveLength(2);
  expect(within(typeTabs).queryByRole("tab", { name: "全局指令" })).not.toBeInTheDocument();
  await user.click(within(typeTabs).getByRole("tab", { name: "Skills" }));
  await user.keyboard("{End}");
  expect(within(typeTabs).getByRole("tab", { name: "MCP" })).toHaveFocus();
  expect(within(dialog).getByRole("tabpanel", { name: "MCP" })).toBeInTheDocument();
  expect(prepareExtensionPlanMock).not.toHaveBeenCalled();
});
