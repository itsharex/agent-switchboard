import { expect, it } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ExtensionDiagnostic, ObservedExtension } from "../../api/client";
import {
  applyExtensionPlanMock, discoverExtensionsMock, historyRecord, importDiscoveredSkillMock,
  prepareExtensionRepairMock, renderPage,
  takeoverDiscoveredExtensionMock,
} from "../../test/extensions-page";

function observed(id: string, kind: "skill" | "mcp" = "mcp"): ObservedExtension {
  return { observationId: id, kind, client: "codex", name: id,
    origin: { origin: "userRoot" }, managed: false, contentDigest: "a".repeat(64),
    actions: { import: { supported: true }, takeover: { supported: true } } };
}

function scan(rows: ObservedExtension[], diagnostics: ExtensionDiagnostic[] = []) {
  return { scanId: "scan-native", scannedAt: "2026-09-12T00:00:00Z", observations: rows, diagnostics };
}

async function openDiscovery(user: ReturnType<typeof userEvent.setup>, kind = "MCP") {
  await user.click(await screen.findByRole("tab", { name: kind }));
  await user.click(screen.getByRole("button", { name: "从本机发现" }));
  return screen.findByRole("region", { name: "从本机发现" });
}

it("imports a selected native server directly and keeps the existing installation", async () => {
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-docs")]));
  takeoverDiscoveredExtensionMock.mockResolvedValue({ id: "new", name: "native-docs", revision: 1 });
  const user = userEvent.setup();
  renderPage();
  const dialog = await openDiscovery(user);
  const selection = await within(dialog).findByRole("checkbox", { name: "选择 native-docs 的 Codex 安装" });
  expect(selection).toBeChecked();
  expect(within(dialog).getByText("保留现有安装")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "导入所选（1）" }));
  await waitFor(() => expect(takeoverDiscoveredExtensionMock).toHaveBeenCalledWith("native-docs"));
  await waitFor(() => expect(discoverExtensionsMock).toHaveBeenCalledTimes(2));
  expect(screen.queryByRole("dialog", { name: /管理现有安装：/ })).not.toBeInTheDocument();
});

it("closing or deselecting the import list does not change the library", async () => {
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-docs")]));
  const user = userEvent.setup();
  renderPage();
  const dialog = await openDiscovery(user);
  await user.click(await within(dialog).findByRole("checkbox", { name: "全选" }));
  expect(within(dialog).getByRole("button", { name: "导入所选（0）" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(takeoverDiscoveredExtensionMock).not.toHaveBeenCalled();
  expect(screen.queryByRole("region", { name: "从本机发现" })).not.toBeInTheDocument();
});

it("continues a batch after an item fails and reports the exact failed item", async () => {
  const legacy = { ...observed("archived-skill", "skill"), origin: { origin: "legacyRoot" as const },
    actions: { import: { supported: true }, takeover: { supported: false } } };
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-skill", "skill"), legacy]));
  takeoverDiscoveredExtensionMock.mockRejectedValue({ code: "observation-stale", message: "安装内容已变更" });
  importDiscoveredSkillMock.mockResolvedValue({ id: "copied", name: "archived-skill", revision: 1 });
  const user = userEvent.setup();
  renderPage();
  const dialog = await openDiscovery(user, "Skills");
  await user.click(await within(dialog).findByRole("button", { name: "导入所选（2）" }));
  await waitFor(() => expect(importDiscoveredSkillMock).toHaveBeenCalledWith("archived-skill"));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("native-skill：安装内容已变更");
  await waitFor(() => expect(discoverExtensionsMock).toHaveBeenCalledTimes(2));
});

it("blocks a repeated import while the first selected batch is running", async () => {
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-docs")]));
  let finish!: () => void;
  takeoverDiscoveredExtensionMock.mockImplementation(() => new Promise((resolve) => {
    finish = () => resolve({ id: "new", name: "native-docs", revision: 1 });
  }));
  const user = userEvent.setup();
  renderPage();
  const dialog = await openDiscovery(user);
  const button = await within(dialog).findByRole("button", { name: "导入所选（1）" });
  fireEvent.click(button);
  fireEvent.click(button);
  expect(takeoverDiscoveredExtensionMock).toHaveBeenCalledTimes(1);
  finish();
  await waitFor(() => expect(discoverExtensionsMock).toHaveBeenCalledTimes(2));
});

it("keeps discovery search and tabs separate from the library", async () => {
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-docs")]));
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: "管理 接口规范" });
  await user.type(screen.getByRole("searchbox", { name: "搜索扩展" }), "接口");
  await user.click(screen.getByRole("button", { name: "从本机发现" }));
  const panel = screen.getByRole("region", { name: "从本机发现" });
  expect(within(panel).getByRole("searchbox", { name: "搜索扩展" })).toHaveValue("");
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  expect(within(panel).getByRole("searchbox", { name: "搜索扩展" })).toHaveValue("");
  await user.type(within(panel).getByRole("searchbox", { name: "搜索扩展" }), "native");
  await user.click(screen.getByRole("tab", { name: "Skills" }));
  await user.click(screen.getByRole("button", { name: "返回扩展库" }));
  expect(screen.getByRole("tab", { name: "Skills" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("searchbox", { name: "搜索扩展" })).toHaveValue("接口");
});

it("scopes warnings and one-click repair to the current discovery tab", async () => {
  const skillDiagnostic: ExtensionDiagnostic = { id: "diag-skill", code: "managedTargetMissing", client: "codex",
    subject: { kind: "managedBinding", bindingId: "bind-1" }, message: "Skill 目录缺失",
    remediation: { kind: "auto", reason: "本地内容库保存了已部署版本" } };
  const mcpDiagnostic: ExtensionDiagnostic = { id: "diag-mcp", code: "managedEntryMissing", client: "codex",
    subject: { kind: "discoveryEntry", observationId: "native-docs" }, message: "MCP 条目缺失",
    remediation: { kind: "auto", reason: "基线保存了服务条目" } };
  discoverExtensionsMock.mockResolvedValue(scan([observed("native-docs")], [skillDiagnostic, mcpDiagnostic]));
  prepareExtensionRepairMock.mockResolvedValue({ planId: "repair", createdAt: "2026-09-12T00:01:00Z",
    expiresAt: "2026-09-12T00:16:00Z", operations: [] });
  applyExtensionPlanMock.mockResolvedValue({ record: historyRecord, rejected: null, rolledBack: false });
  const user = userEvent.setup();
  renderPage();
  const panel = await openDiscovery(user, "Skills");
  expect(await within(panel).findByText("当前结果有 1 条警告，1 条可修复")).toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  expect(discoverExtensionsMock).toHaveBeenCalledTimes(1);
  const mcpPanel = screen.getByRole("region", { name: "从本机发现" });
  await user.click(within(mcpPanel).getByRole("button", { name: "修复这 1 项可修复警告" }));
  await waitFor(() => expect(prepareExtensionRepairMock).toHaveBeenCalledWith("scan-native", ["diag-mcp"]));
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("repair", true));
  await waitFor(() => expect(discoverExtensionsMock).toHaveBeenCalledTimes(2));
});
