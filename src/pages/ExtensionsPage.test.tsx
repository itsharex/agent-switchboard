import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ExtensionsPage } from "./ExtensionsPage";
import {
  applyExtensionPlan,
  checkMcpConnection,
  checkSkillUpdates,
  discoverExtensions,
  exportExtensionPortable,
  getMcpCheck,
  getMcpEditView,
  importExtensionPortable,
  listExtensions,
  prepareExtensionPlan,
  prepareExtensionRepair,
  previewDiscoveredTakeover,
  recoverExtensionTransactions,
  setBindingLock,
  takeoverDiscoveredExtension,
  updateMcpDefinition,
  updateSkillDefinition,
  type ClientCapabilityReport,
  type ExtensionDiagnostic,
  type ExtensionListItem,
  type ExtensionOperationRecord,
  type ExtensionPlanView,
  type ExtensionsWorkspace,
  type McpCheckResult,
  type McpEditViewEnvelope,
} from "../api/client";

vi.mock("../api/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/client")>();
  return {
    ...actual,
    listExtensions: vi.fn(),
    prepareExtensionPlan: vi.fn(),
    prepareExtensionRepair: vi.fn(),
    applyExtensionPlan: vi.fn(),
    recoverExtensionTransactions: vi.fn(),
    checkMcpConnection: vi.fn(),
    getMcpCheck: vi.fn(),
    checkSkillUpdates: vi.fn(),
    updateSkillDefinition: vi.fn(),
    getMcpEditView: vi.fn(),
    updateMcpDefinition: vi.fn(),
    setBindingLock: vi.fn(),
    discoverExtensions: vi.fn(),
    previewDiscoveredTakeover: vi.fn(),
    takeoverDiscoveredExtension: vi.fn(),
    exportExtensionPortable: vi.fn(),
    importExtensionPortable: vi.fn(),
  };
});

const listExtensionsMock = vi.mocked(listExtensions);
const prepareExtensionPlanMock = vi.mocked(prepareExtensionPlan);
const prepareExtensionRepairMock = vi.mocked(prepareExtensionRepair);
const applyExtensionPlanMock = vi.mocked(applyExtensionPlan);
const recoverExtensionTransactionsMock = vi.mocked(recoverExtensionTransactions);
const checkMcpConnectionMock = vi.mocked(checkMcpConnection);
const getMcpCheckMock = vi.mocked(getMcpCheck);
const checkSkillUpdatesMock = vi.mocked(checkSkillUpdates);
const updateSkillDefinitionMock = vi.mocked(updateSkillDefinition);
const getMcpEditViewMock = vi.mocked(getMcpEditView);
const updateMcpDefinitionMock = vi.mocked(updateMcpDefinition);
const setBindingLockMock = vi.mocked(setBindingLock);
const discoverExtensionsMock = vi.mocked(discoverExtensions);
const previewDiscoveredTakeoverMock = vi.mocked(previewDiscoveredTakeover);
const takeoverDiscoveredExtensionMock = vi.mocked(takeoverDiscoveredExtension);
const exportExtensionPortableMock = vi.mocked(exportExtensionPortable);
const importExtensionPortableMock = vi.mocked(importExtensionPortable);

const skillItem: ExtensionListItem = {
  schemaVersion: 2,
  id: "ext-skill-1",
  name: "接口规范",
  revision: 1,
  createdAt: "2026-09-01T08:00:00Z",
  updatedAt: "2026-09-01T08:00:00Z",
  kind: "skill",
  contentDigest: "a".repeat(64),
  manifest: { name: "api-spec", description: "撰写并检查接口规范文档", unparsedKeys: [] },
  source: null,
  hostScoped: null,
  compatibility: [{ code: "host-scoped-frontmatter", message: "包含 Claude 专属字段" }],
  dependencies: [],
  bindings: [
    {
      schemaVersion: 2,
      id: "bind-1",
      resourceId: "ext-skill-1",
      target: { scope: "app", client: "codex" },
      desired: "enabled",
      updatedAt: "2026-09-01T08:00:00Z",
      fileState: "inSync",
      warnings: [],
    },
  ],
  dependencyStates: [{ name: "docs", resourceId: "ext-mcp-1", state: "bound" }],
  lastCheck: null,
};

const mcpItem: ExtensionListItem = {
  schemaVersion: 2,
  id: "ext-mcp-1",
  name: "docs",
  revision: 1,
  createdAt: "2026-09-01T08:00:00Z",
  updatedAt: "2026-09-01T08:00:00Z",
  kind: "mcp",
  transport: "stdio",
  command: "npx",
  argumentCount: 2,
  env: { DOCS_TOKEN: { mode: "stored" } },
  bindings: [],
  dependencyStates: [],
  lastCheck: null,
};

const historyRecord: ExtensionOperationRecord = {
  schemaVersion: 3,
  id: "op-1",
  createdAt: "2026-09-02T08:00:00Z",
  finishedAt: "2026-09-02T08:00:05Z",
  resources: [
    {
      definitionId: "ext-mcp-1",
      definitionRevision: 1,
      operation: "install",
      targets: [
        { target: { scope: "app", client: "codex" }, outcome: { kind: "applied" }, warnings: [] },
      ],
    },
  ],
  rollback: null,
};

const capabilityReport: ClientCapabilityReport = {
  client: "codex",
  entries: [
    {
      code: "skill-user",
      resource: "用户级 Skill 目录",
      supported: true,
      verification: "verified",
      clientVersion: "0.42.0",
      verifiedOn: "2026-09-01",
    },
    {
      code: "mcp-project",
      resource: "项目级 MCP 文档",
      supported: false,
      verification: "open",
      condition: "需要隔离环境验证",
    },
  ],
};

const workspace: ExtensionsWorkspace = {
  generation: 1,
  items: [skillItem, mcpItem],
  projects: [
    {
      schemaVersion: 2,
      id: "proj-1",
      root: "D:/works/demo",
      displayName: "demo",
      registeredAt: "2026-09-01T08:00:00Z",
    },
  ],
  history: [historyRecord],
  capabilities: [capabilityReport],
  recoveryRequired: [],
};

const planView: ExtensionPlanView = {
  planId: "plan-1",
  createdAt: "2026-09-03T08:00:00Z",
  expiresAt: "2026-09-03T08:15:00Z",
  operations: [
    {
      definitionId: "ext-mcp-1",
      definitionRevision: 1,
      operation: "install",
      targets: [
        {
          target: { scope: "app", client: "codex" },
          warnings: ["示例警告：将新增服务条目"],
          changes: [{ pointer: "mcp_servers.docs", before: null, after: '{"command":"npx"}' }],
          files: [],
          writesSensitiveConnectionData: true,
        },
      ],
    },
  ],
};

const checkResult: McpCheckResult = {
  definitionId: "ext-mcp-1",
  definitionRevision: 1,
  target: { scope: "app", client: "codex" },
  checkedAt: "2026-09-03T08:01:00Z",
  protocolVersion: "2025-06-18",
  outcome: { kind: "passed", tools: 3, resources: 1, prompts: 2 },
  durationMs: 812,
  truncated: false,
};

function renderPage() {
  return render(
    <ExtensionsPage busy={false} setBusy={vi.fn()} clearError={vi.fn()} onError={vi.fn()} />,
  );
}

async function openMcpDetail(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("tab", { name: "MCP" }));
  await user.click(await screen.findByRole("button", { name: "管理 docs" }));
  return screen.findByRole("region", { name: "扩展详情 docs" });
}

beforeEach(() => {
  vi.mocked(listExtensions).mockReset();
  prepareExtensionPlanMock.mockReset();
  applyExtensionPlanMock.mockReset();
  recoverExtensionTransactionsMock.mockReset();
  checkMcpConnectionMock.mockReset();
  getMcpCheckMock.mockReset();
  checkSkillUpdatesMock.mockReset();
  updateSkillDefinitionMock.mockReset();
  getMcpEditViewMock.mockReset();
  updateMcpDefinitionMock.mockReset();
  setBindingLockMock.mockReset();
  listExtensionsMock.mockResolvedValue(workspace);
});

describe("ExtensionsPage", () => {
  it("renders the type tabs, the list with per-client summaries, and the history", async () => {
    renderPage();

    expect(await screen.findByRole("tab", { name: "Skills" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const table = await screen.findByRole("table", { name: "扩展列表" });
    expect(within(table).getByText("接口规范")).toBeInTheDocument();
    expect(within(table).getByText("已一致")).toBeInTheDocument();
    expect(within(table).queryByText("docs")).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: "MCP" }));
    expect(await within(table).findByText("docs")).toBeInTheDocument();
    expect(within(table).getAllByText("未部署").length).toBeGreaterThan(0);

    const history = screen.getByRole("region", { name: "操作历史" });
    expect(within(history).getByText("安装")).toBeInTheDocument();
    expect(within(history).getByText("Codex 用户配置：已应用")).toBeInTheDocument();
    expect(within(history).getByRole("button", { name: "恢复" })).toBeEnabled();
  });

  it("expands the detail below the list with facts, bindings, and capabilities", async () => {
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

    await user.click(within(detail).getByRole("button", { name: "关闭详情" }));
    expect(screen.queryByRole("region", { name: "扩展详情 docs" })).not.toBeInTheDocument();
  });

  it("applies a multi-target install only after the plan sheet confirms with confirmWrite", async () => {
    prepareExtensionPlanMock.mockResolvedValue(planView);
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

    const sheet = await screen.findByRole("dialog", { name: "安装预览" });
    expect(within(sheet).getByText("警告：示例警告：将新增服务条目")).toBeInTheDocument();
    expect(within(sheet).getByText("该目标会写入已脱敏的连接地址、参数或凭据值")).toBeInTheDocument();
    expect(within(sheet).getByText("mcp_servers.docs")).toBeInTheDocument();

    await user.click(within(sheet).getByRole("button", { name: "确认应用" }));

    await waitFor(() => expect(applyExtensionPlanMock).toHaveBeenCalledTimes(1));
    expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-1", true);
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "安装预览" })).not.toBeInTheDocument(),
    );
  });

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

    expect(checkMcpConnectionMock).toHaveBeenCalledWith(
      "ext-mcp-1",
      { scope: "app", client: "codex" },
      true,
    );
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

  it("updates a Skill into the library and then requires deployment confirmation", async () => {
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

    await waitFor(() =>
      expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-1", "b".repeat(64)),
    );
    expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
      operations: [{ operation: "update", definitionId: "ext-skill-1" }],
    });
    expect(applyExtensionPlanMock).not.toHaveBeenCalled();
    expect(await screen.findByRole("dialog", { name: "更新预览" })).toBeInTheDocument();
  });

  it("checks updates for a multi-selection and previews one combined update plan", async () => {
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

    await user.click(await screen.findByRole("checkbox", { name: "选择 接口规范" }));
    await user.click(screen.getByRole("checkbox", { name: "选择 文档检索" }));

    const batchbar = screen.getByRole("toolbar", { name: "批量操作" });
    expect(within(batchbar).getByText("已选择 2 个 Skill")).toBeInTheDocument();
    await user.click(within(batchbar).getByRole("button", { name: "检查更新" }));

    expect(checkSkillUpdatesMock).toHaveBeenCalledWith(["ext-skill-1", "ext-skill-2"]);
    expect(await screen.findByText("批量更新检查")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "预览批量更新（2）" }));

    await waitFor(() =>
      expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-1", "b".repeat(64)),
    );
    await waitFor(() =>
      expect(updateSkillDefinitionMock).toHaveBeenCalledWith("ext-skill-2", "d".repeat(64)),
    );
    expect(prepareExtensionPlanMock).toHaveBeenCalledWith({
      operations: [
        { operation: "update", definitionId: "ext-skill-1" },
        { operation: "update", definitionId: "ext-skill-2" },
      ],
    });
    expect(applyExtensionPlanMock).not.toHaveBeenCalled();
    expect(await screen.findByRole("dialog", { name: "更新预览" })).toBeInTheDocument();
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
    listExtensionsMock
      .mockResolvedValueOnce(workspace)
      .mockResolvedValueOnce(lockedWorkspace);
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
    await waitFor(() =>
      expect(screen.queryByRole("form", { name: "编辑 MCP 服务" })).not.toBeInTheDocument(),
    );
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
    await waitFor(() =>
      expect(screen.queryByRole("form", { name: "编辑 MCP 服务" })).not.toBeInTheDocument(),
    );
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

  it("shows the empty state with both entry cards when the library is empty", async () => {
    listExtensionsMock.mockResolvedValue({
      generation: 0,
      items: [],
      projects: [],
      history: [],
      capabilities: [],
      recoveryRequired: [],
    });
    renderPage();

    expect(
      await screen.findByText("扩展库为空；从下方入口新建或从本机导入扩展"),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /添加 Skill 来源/ })).toHaveLength(2);
    expect(screen.getByRole("button", { name: /从本机发现/ })).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "扩展列表" })).not.toBeInTheDocument();
  });

  it("lets the user run recovery for a pending extension transaction", async () => {
    listExtensionsMock.mockResolvedValue({ ...workspace, recoveryRequired: ["op-recover"] });
    recoverExtensionTransactionsMock.mockResolvedValue([
      "op-recover：客户端与扩展库均已恢复到操作前状态",
    ]);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "尝试恢复" }));

    await waitFor(() => expect(recoverExtensionTransactionsMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(listExtensionsMock).toHaveBeenCalledTimes(2));
  });

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
      warnings: [
        "接管不改写客户端文件；本机现有配置保持原样",
        "移除该绑定时，将恢复接管时的原生条目",
      ],
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
    await user.click(await screen.findByRole("button", { name: "管理现有安装" }));

    const dialog = await screen.findByRole("dialog", { name: "管理现有安装：native-docs" });
    // The panel states the three things the user must understand.
    expect(within(dialog).getByText(/管理对象/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Codex · Codex 用户级配置/)).toBeInTheDocument();
    expect(within(dialog).getByText(/现有文件保持原样/)).toBeInTheDocument();
    expect(within(dialog).getByText(/移除该绑定时，将恢复接管时的原生条目/)).toBeInTheDocument();
    expect(
      within(dialog).getByText("管理现有安装已包含加入扩展库，无需先复制。"),
    ).toBeInTheDocument();
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
      expect(prepareExtensionRepairMock).toHaveBeenCalledWith("scan-repair-1", [
        "diag-missing",
      ]),
    );
    const dialog = await screen.findByRole("dialog", { name: "修复预览" });
    expect(within(dialog).getByText(/将恢复最后一次部署的内容版本/)).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "确认应用" }));
    await waitFor(() =>
      expect(applyExtensionPlanMock).toHaveBeenCalledWith("plan-repair-1", true),
    );
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
    expect(
      screen.getByText("提示：管理现有安装已包含加入扩展库，无需先复制。"),
    ).toBeInTheDocument();

    // The repair batch names exactly the objects the user sees: the visible
    // MCP entry, not the skill warning from the other tab.
    prepareExtensionRepairMock.mockResolvedValue({
      planId: "plan-scope",
      createdAt: "2026-09-07T02:01:00.000Z",
      expiresAt: "2026-09-07T02:16:00.000Z",
      operations: [],
    });
    await user.click(screen.getByRole("button", { name: "修复这 1 项可修复警告" }));
    await waitFor(() =>
      expect(prepareExtensionRepairMock).toHaveBeenCalledWith("scan-scope-1", ["diag-mcp"]),
    );
  });

  it("imports a portable package and reports missing credential slots", async () => {
    importExtensionPortableMock.mockResolvedValue({
      definition: { id: "ext-imported", name: "docs", revision: 1 },
      missingEnvSlots: ["TOKEN"],
      warnings: [],
    });
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "导入便携包" }));
    const form = screen.getByRole("form", { name: "导入便携包" });
    await user.type(within(form).getByLabelText(/便携包文件路径/), "D:\\pkgs\\docs.json");
    await user.click(within(form).getByRole("button", { name: "导入便携包" }));

    await waitFor(() =>
      expect(importExtensionPortableMock).toHaveBeenCalledWith("D:\\pkgs\\docs.json"),
    );
    // The import finished and the form closed with the definition selected.
    await waitFor(() =>
      expect(screen.queryByRole("form", { name: "导入便携包" })).not.toBeInTheDocument(),
    );
  });

  it("exports a portable package from the detail view with a chosen path", async () => {
    exportExtensionPortableMock.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderPage();
    const detail = await openMcpDetail(user);

    await user.click(within(detail).getByRole("button", { name: "导出便携包" }));
    const dialog = await screen.findByRole("dialog", { name: "导出便携包 docs" });
    expect(within(dialog).getByText(/不含密钥、服务地址或本机路径/)).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("导出文件路径"), "D:\\docs-portable.json");
    await user.click(within(dialog).getByRole("button", { name: "导出到文件" }));

    await waitFor(() =>
      expect(exportExtensionPortableMock).toHaveBeenCalledWith("ext-mcp-1", "D:\\docs-portable.json"),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "导出便携包 docs" })).not.toBeInTheDocument(),
    );
  });
});
