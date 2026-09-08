import { useState } from "react";
import { beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ExtensionsPage } from "../pages/ExtensionsPage";
import type { ExtensionSection } from "../app/navigation";
import {
  applyExtensionPlan,
  checkMcpConnection,
  checkSkillUpdates,
  discoverExtensions,
  exportExtensionPortable,
  getGlobalPromptDocument,
  getMcpCheck,
  getMcpEditView,
  importExtensionPortable,
  listExtensions,
  prepareExtensionPlan,
  prepareExtensionRepair,
  previewDiscoveredTakeover,
  recoverExtensionTransactions,
  saveGlobalPromptDocument,
  setBindingLock,
  takeoverDiscoveredExtension,
  updateMcpDefinition,
  updateSkillDefinition,
  type ClientCapabilityReport,
  type ExtensionListItem,
  type ExtensionOperationRecord,
  type ExtensionPlanView,
  type ExtensionsWorkspace,
  type McpCheckResult,
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
    getGlobalPromptDocument: vi.fn(),
    saveGlobalPromptDocument: vi.fn(),
  };
});

export const listExtensionsMock = vi.mocked(listExtensions);
export const prepareExtensionPlanMock = vi.mocked(prepareExtensionPlan);
export const prepareExtensionRepairMock = vi.mocked(prepareExtensionRepair);
export const applyExtensionPlanMock = vi.mocked(applyExtensionPlan);
export const recoverExtensionTransactionsMock = vi.mocked(recoverExtensionTransactions);
export const checkMcpConnectionMock = vi.mocked(checkMcpConnection);
export const getMcpCheckMock = vi.mocked(getMcpCheck);
export const checkSkillUpdatesMock = vi.mocked(checkSkillUpdates);
export const updateSkillDefinitionMock = vi.mocked(updateSkillDefinition);
export const getMcpEditViewMock = vi.mocked(getMcpEditView);
export const updateMcpDefinitionMock = vi.mocked(updateMcpDefinition);
export const setBindingLockMock = vi.mocked(setBindingLock);
export const discoverExtensionsMock = vi.mocked(discoverExtensions);
export const previewDiscoveredTakeoverMock = vi.mocked(previewDiscoveredTakeover);
export const takeoverDiscoveredExtensionMock = vi.mocked(takeoverDiscoveredExtension);
export const exportExtensionPortableMock = vi.mocked(exportExtensionPortable);
export const importExtensionPortableMock = vi.mocked(importExtensionPortable);
export const getGlobalPromptDocumentMock = vi.mocked(getGlobalPromptDocument);
export const saveGlobalPromptDocumentMock = vi.mocked(saveGlobalPromptDocument);

export const skillItem: Extract<ExtensionListItem, { kind: "skill" }> = {
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

export const mcpItem: Extract<ExtensionListItem, { kind: "mcp"; transport: "stdio" }> = {
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

export const historyRecord: ExtensionOperationRecord = {
  schemaVersion: 3,
  id: "op-1",
  createdAt: "2026-09-02T08:00:00Z",
  finishedAt: "2026-09-02T08:00:05Z",
  resources: [
    {
      definitionId: "ext-mcp-1",
      definitionRevision: 1,
      operation: "install",
      targets: [{ target: { scope: "app", client: "codex" }, outcome: { kind: "applied" }, warnings: [] }],
    },
  ],
  rollback: null,
};

export const capabilityReport: ClientCapabilityReport = {
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

export const workspace: ExtensionsWorkspace = {
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

export const planView: ExtensionPlanView = {
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

export const checkResult: McpCheckResult = {
  definitionId: "ext-mcp-1",
  definitionRevision: 1,
  target: { scope: "app", client: "codex" },
  checkedAt: "2026-09-03T08:01:00Z",
  protocolVersion: "2025-06-18",
  outcome: { kind: "passed", tools: 3, resources: 1, prompts: 2 },
  durationMs: 812,
  truncated: false,
};

function TestExtensionsPage() {
  const [section, setSection] = useState<ExtensionSection>("skill");
  return <ExtensionsPage busy={false} setBusy={vi.fn()} clearError={vi.fn()} onError={vi.fn()}
    section={section} onSectionChange={setSection} instructions={<p>全局指令编辑区</p>} />;
}

export function renderPage() {
  return render(<TestExtensionsPage />);
}

export async function openMcpDetail(user: ReturnType<typeof userEvent.setup>) {
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
  getGlobalPromptDocumentMock.mockReset();
  saveGlobalPromptDocumentMock.mockReset();
  listExtensionsMock.mockResolvedValue(workspace);
});
