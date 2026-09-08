import { expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ExtensionDiagnostic } from "../../api/client";
import {
  prepareExtensionRepairMock,
  applyExtensionPlanMock,
  discoverExtensionsMock,
  previewDiscoveredTakeoverMock,
  takeoverDiscoveredExtensionMock,
  historyRecord,
  renderPage,
} from "../../test/extensions-page";

it("previews and confirms a takeover of a discovered native MCP", async () => {
  discoverExtensionsMock.mockResolvedValue({
    scanId: "scan-takeover-1",
    scannedAt: "2026-09-07T00:00:00.000Z",
    observations: [
      {
        observationId: "obs-take-1",
        kind: "mcp",
        client: "codex",
        name: "native-docs",
        origin: { origin: "userRoot" },
        managed: false,
        transport: "stdio",
        actions: {
          import: { supported: true, inLibrary: false },
          takeover: { supported: true },
        },
      },
    ],
    diagnostics: [],
  });
  previewDiscoveredTakeoverMock.mockResolvedValue({
    kind: "mcp",
    name: "native-docs",
    client: "codex",
    scopeLabel: "Codex 用户级配置",
    nativeEntryPresent: true,
    fileCount: null,
    contentDigest: null,
    definitionExists: false,
    warnings: ["接管不改写客户端文件；本机现有配置保持原样", "移除该绑定时，将恢复接管时的原生条目"],
  });
  takeoverDiscoveredExtensionMock.mockResolvedValue({
    id: "ext-new",
    name: "native-docs",
    revision: 1,
  });
  const user = userEvent.setup();
  renderPage();

  // The discovery results follow the active tab: an MCP row only shows
  // under the MCP tab.
  await user.click(await screen.findByRole("tab", { name: "MCP" }));
  await user.click(await screen.findByRole("button", { name: /从本机发现/ }));
  const discovery = await screen.findByRole("dialog", { name: "从本机发现" });
  for (const [name, panelId] of [
    ["Skills", "ext-discovery-skill-panel"],
    ["MCP", "ext-discovery-mcp-panel"],
  ]) {
    const tab = within(discovery).getByRole("tab", { name });
    const panel = document.getElementById(panelId)!;
    expect(tab).toHaveAttribute("aria-controls", panelId);
    expect(panel).toHaveAttribute("role", "tabpanel");
    expect(panel).toHaveAttribute("aria-labelledby", tab.id);
  }
  expect(document.getElementById("ext-discovery-skill-panel")!).toHaveAttribute("hidden");
  await user.click(await screen.findByRole("button", { name: "管理现有安装" }));

  const dialog = await screen.findByRole("dialog", { name: "管理现有安装：native-docs" });
  // The panel states the three things the user must understand.
  expect(within(dialog).getByText(/管理对象/)).toBeInTheDocument();
  expect(within(dialog).getByText(/Codex · Codex 用户级配置/)).toBeInTheDocument();
  expect(within(dialog).getByText(/现有文件保持原样/)).toBeInTheDocument();
  expect(within(dialog).getByText(/移除该绑定时，将恢复接管时的原生条目/)).toBeInTheDocument();
  expect(within(dialog).getByText("管理现有安装已包含加入扩展库，无需先复制。")).toBeInTheDocument();
  expect(previewDiscoveredTakeoverMock).toHaveBeenCalledWith("obs-take-1");

  await user.click(within(dialog).getByRole("button", { name: "确认管理" }));
  await waitFor(() => expect(takeoverDiscoveredExtensionMock).toHaveBeenCalledWith("obs-take-1"));
});

it("prepares a one-click repair from the scan and applies it after confirmation", async () => {
  discoverExtensionsMock.mockReset();
  const missingDiagnostic: ExtensionDiagnostic = {
    id: "diag-missing",
    code: "managedTargetMissing",
    client: "codex",
    subject: { kind: "managedBinding", bindingId: "bind-1" },
    message: "托管 Skill 目录 接口规范 已缺失",
    remediation: { kind: "auto", reason: "本地内容库保存了已部署版本，可整体恢复" },
  };
  discoverExtensionsMock.mockResolvedValue({
    scanId: "scan-repair-1",
    scannedAt: "2026-09-07T00:00:00.000Z",
    observations: [],
    diagnostics: [missingDiagnostic],
  });
  prepareExtensionRepairMock.mockResolvedValue({
    planId: "plan-repair-1",
    createdAt: "2026-09-07T00:01:00.000Z",
    expiresAt: "2026-09-07T00:16:00.000Z",
    operations: [
      {
        definitionId: "ext-skill-1",
        definitionRevision: 1,
        operation: "repair",
        targets: [
          {
            target: { scope: "app", client: "codex" },
            warnings: ["将恢复最后一次部署的内容版本；版本与停用状态保持不变"],
            changes: [],
            files: [{ relativePath: "SKILL.md", action: "deploy" }],
            writesSensitiveConnectionData: false,
          },
        ],
      },
    ],
  });
  applyExtensionPlanMock.mockResolvedValue({
    record: { ...historyRecord, id: "op-repair" },
    rejected: null,
    rolledBack: false,
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("button", { name: /从本机发现/ }));
  // The collapsed summary is one counted line with the repair action.
  expect(screen.getByText("当前结果有 1 条警告，1 条可修复")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "修复这 1 项可修复警告" }));

  await waitFor(() =>
    expect(prepareExtensionRepairMock).toHaveBeenCalledWith("scan-repair-1", ["diag-missing"]),
  );
  const dialog = await screen.findByRole("dialog", { name: "修复预览" });
  expect(within(dialog).getByText(/将恢复最后一次部署的内容版本/)).toBeInTheDocument();

  await user.click(within(dialog).getByRole("button", { name: "确认应用" }));
  await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-repair-1", true));
  // After the confirmed repair the workspace refreshes and re-scans, so
  // the fresh diagnostics decide what disappeared.
  await waitFor(() => expect(discoverExtensionsMock).toHaveBeenCalledTimes(2));
});

it("scopes discovery rows, warnings, and the repair batch to the active tab", async () => {
  discoverExtensionsMock.mockReset();
  const skillDiagnostic: ExtensionDiagnostic = {
    id: "diag-skill",
    code: "managedTargetMissing",
    client: "codex",
    subject: { kind: "managedBinding", bindingId: "bind-1" },
    message: "托管 Skill 目录 接口规范 已缺失",
    remediation: { kind: "auto", reason: "本地内容库保存了已部署版本" },
  };
  const mcpDiagnostic: ExtensionDiagnostic = {
    id: "diag-mcp",
    code: "managedEntryMissing",
    client: "codex",
    subject: { kind: "discoveryEntry", observationId: "obs-mcp-native" },
    message: "托管服务条目 mcpServers.native-docs 不在当前配置中",
    remediation: { kind: "auto", reason: "基线保存了最后一次写入的服务条目" },
  };
  discoverExtensionsMock.mockResolvedValue({
    scanId: "scan-scope-1",
    scannedAt: "2026-09-07T02:00:00.000Z",
    observations: [
      {
        observationId: "obs-mcp-native",
        kind: "mcp",
        client: "codex",
        name: "native-docs",
        origin: { origin: "userRoot" },
        managed: false,
        transport: "stdio",
        actions: {
          import: { supported: true, inLibrary: false },
          takeover: { supported: true },
        },
      },
    ],
    diagnostics: [skillDiagnostic, mcpDiagnostic],
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(await screen.findByRole("button", { name: /从本机发现/ }));
  // Skills tab: the MCP entry diagnostic is out of view, the managed skill
  // warning is counted, and the copy hint is visible.
  expect(screen.getByText("当前结果有 1 条警告，1 条可修复")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "管理现有安装" })).not.toBeInTheDocument();
  expect(screen.getByText(/当前 Skills 筛选范围下没有本机发现结果/)).toBeInTheDocument();

  // Switching to the MCP tab reuses the same scan and shows the MCP row
  // with the full action pair.
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  expect(discoverExtensionsMock).toHaveBeenCalledTimes(1);
  expect(await screen.findByText("native-docs")).toBeInTheDocument();
  expect(screen.getByText("未托管")).toBeInTheDocument();
  expect(screen.getByText("Codex 用户级目录")).toBeInTheDocument();
  expect(screen.getByText("提示：管理现有安装已包含加入扩展库，无需先复制。")).toBeInTheDocument();

  // The repair batch names exactly the objects the user sees: the visible
  // MCP entry, not the skill warning from the other tab.
  prepareExtensionRepairMock.mockResolvedValue({
    planId: "plan-scope",
    createdAt: "2026-09-07T02:01:00.000Z",
    expiresAt: "2026-09-07T02:16:00.000Z",
    operations: [],
  });
  await user.click(screen.getByRole("button", { name: "修复这 1 项可修复警告" }));
  await waitFor(() => expect(prepareExtensionRepairMock).toHaveBeenCalledWith("scan-scope-1", ["diag-mcp"]));
});
