import { useState } from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, vi } from "vitest";
import * as client from "../../api/client";
import type {
  ExtensionListItem,
  ExtensionOperationRecord,
  ExtensionPlanView,
  ExtensionsWorkspace,
  SkillUpdateReport,
} from "../../api/client";
import type { ExtensionSection } from "../../app/navigation";
import { toast } from "../../components/use-toast";
import { useExtensionWorkspace } from "./useExtensionWorkspace";

vi.mock("../../api/client", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../api/client")>(),
  listExtensions: vi.fn(),
  prepareExtensionPlan: vi.fn(),
  prepareExtensionRepair: vi.fn(),
  prepareExtensionRestore: vi.fn(),
  applyExtensionPlan: vi.fn(),
  deleteExtension: vi.fn(),
  saveExtension: vi.fn(),
  getMcpEditView: vi.fn(),
  updateMcpDefinition: vi.fn(),
  putExtensionSecret: vi.fn(),
  checkSkillUpdates: vi.fn(),
  updateSkillDefinition: vi.fn(),
  createLocalSkill: vi.fn(),
  forkLocalSkill: vi.fn(),
  getSkillEditor: vi.fn(),
  listSkillVersions: vi.fn(),
  updateSkillFiles: vi.fn(),
  updateSkillDependencies: vi.fn(),
  setBindingLock: vi.fn(),
  recoverExtensionTransactions: vi.fn(),
  discoverExtensions: vi.fn(),
  importSkillCandidate: vi.fn(),
}));

vi.mock("../../components/use-toast", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../components/use-toast")>(),
  toast: vi.fn(),
}));

export const api = vi.mocked(client);
export const notify = vi.mocked(toast);
export const onError = vi.fn();
export const clearError = vi.fn();

export const mcp: Extract<ExtensionListItem, { kind: "mcp"; transport: "stdio" }> = {
  schemaVersion: 2, id: "mcp", name: "docs", revision: 1,
  createdAt: "2026-09-12T00:00:00Z", updatedAt: "2026-09-12T00:00:00Z",
  kind: "mcp", transport: "stdio", command: "npx", argumentCount: 1, env: {},
  bindings: [], dependencyStates: [], lastCheck: null,
};

export const skill: Extract<ExtensionListItem, { kind: "skill" }> = {
  schemaVersion: 2, id: "skill", name: "rules", revision: 1,
  createdAt: "2026-09-12T00:00:00Z", updatedAt: "2026-09-12T00:00:00Z",
  kind: "skill", contentDigest: "a".repeat(64),
  manifest: { name: "rules", description: "Project rules", unparsedKeys: [] },
  source: null, hostScoped: null, compatibility: [], dependencies: [],
  dependencyStates: [], lastCheck: null,
  bindings: [{
    schemaVersion: 2, id: "binding", resourceId: "skill", target: { scope: "app", client: "codex" },
    desired: "enabled", lastAppliedRevision: 1, fileState: "inSync", warnings: [],
    updatedAt: "2026-09-12T00:00:00Z",
  }],
};

export const record: ExtensionOperationRecord = {
  schemaVersion: 3, id: "operation", createdAt: "2026-09-12T00:00:00Z",
  finishedAt: "2026-09-12T00:00:01Z", rollback: null,
  resources: [{ definitionId: "skill", definitionRevision: 1, operation: "install",
    targets: [{ target: { scope: "app", client: "codex" }, outcome: { kind: "applied" } }] }],
};

export const workspace: ExtensionsWorkspace = {
  generation: 1, items: [mcp, skill], history: [record], capabilities: [], projects: [], recoveryRequired: [],
};

export const updateReport: SkillUpdateReport = {
  definitionId: "skill", upToDate: false, currentCommit: null, newCommit: null,
  newDigest: "b".repeat(64), changedFiles: [], error: null,
};

export function prepared(sensitive = false): ExtensionPlanView {
  return {
    planId: "plan", createdAt: "2026-09-12T00:00:00Z", expiresAt: "2026-09-12T00:15:00Z",
    operations: [{ definitionId: "skill", definitionRevision: 1, operation: "update",
      targets: [{ target: { scope: "app", client: "codex" }, warnings: [], changes: [],
        writesSensitiveConnectionData: sensitive }] }],
  };
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

export function useTestExtensionWorkspace() {
  const [busy, setBusy] = useState(false);
  const [section, onSectionChange] = useState<ExtensionSection>("skill");
  return useExtensionWorkspace({ busy, setBusy, section, onSectionChange, clearError, onError });
}

export async function renderOperationWorkspace() {
  const view = renderHook(useTestExtensionWorkspace);
  await waitFor(() => { if (!view.result.current.ext.loaded) throw new Error("Workspace not loaded"); });
  return view;
}

beforeEach(() => {
  vi.resetAllMocks();
  api.listExtensions.mockResolvedValue(workspace);
  api.prepareExtensionPlan.mockResolvedValue(prepared());
  api.prepareExtensionRestore.mockResolvedValue(prepared());
  api.prepareExtensionRepair.mockResolvedValue(prepared());
  api.applyExtensionPlan.mockResolvedValue({ record, rejected: null, rolledBack: false });
  api.deleteExtension.mockResolvedValue(undefined);
  api.saveExtension.mockResolvedValue({ id: "mcp", name: "docs", revision: 1 });
  api.updateSkillDefinition.mockResolvedValue({ id: "skill", name: "rules", revision: 2 });
  api.discoverExtensions.mockResolvedValue({ scanId: "scan", scannedAt: "2026-09-12T00:00:00Z",
    observations: [], diagnostics: [] });
});
