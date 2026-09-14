import { act, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { SkillEditorView } from "../api/client";
import {
  api, deferred, mcp, notify, onError, renderOperationWorkspace, skill, workspace,
} from "../pages/extensions/extension-apply.test-support";

it("uses the released lock even when a continuation captured the busy render", async () => {
  const { result } = await renderOperationWorkspace();
  const held = deferred<string>();
  let first!: Promise<string | null>;
  act(() => { first = result.current.ext.runExclusive(() => held.promise); });
  await waitFor(() => expect(result.current.busy).toBe(true));
  const continueFromBusy = result.current.ext.runExclusive;
  const blocked = vi.fn(async () => "unexpected");
  await act(async () => {
    expect(await continueFromBusy(blocked)).toBeNull();
    held.resolve("first");
    expect(await first).toBe("first");
    expect(await continueFromBusy(async () => "second")).toBe("second");
  });
  expect(blocked).not.toHaveBeenCalled();
  expect(result.current.busy).toBe(false);
});

it("allows a refreshed editor read without releasing the active write lock", async () => {
  const editor: SkillEditorView = { id: "skill", revision: 2, contentDigest: "b".repeat(64),
    manifest: skill.manifest, editable: true, files: [] };
  api.getSkillEditor.mockResolvedValue(editor);
  const { result } = await renderOperationWorkspace();
  const held = deferred<boolean>();
  let first!: Promise<boolean | null>;
  act(() => { first = result.current.ext.runExclusive(() => held.promise); });
  const loadEditor = result.current.ext.loadSkillEditor;
  await act(async () => { expect(await loadEditor("skill")).toEqual(editor); });
  expect(result.current.busy).toBe(true);
  expect(result.current.ext.loadSkillEditor).toBe(loadEditor);
  await act(async () => { held.resolve(true); await first; });
  expect(result.current.ext.loadSkillEditor).toBe(loadEditor);
});

it("reports an action error and makes the next action available", async () => {
  const { result } = await renderOperationWorkspace();
  const error = { code: "write", message: "Database unavailable" };
  await act(async () => {
    expect(await result.current.ext.runExclusive(async () => { throw error; })).toBeNull();
    expect(await result.current.ext.runExclusive(async () => true)).toBe(true);
  });
  expect(onError).toHaveBeenCalledWith(error);
  expect(result.current.busy).toBe(false);
});

it("marks a saved MCP edit unverified instead of declaring no deployment on refresh failure", async () => {
  api.updateMcpDefinition.mockResolvedValue({ id: "mcp", name: "docs", revision: 2 });
  const { result } = await renderOperationWorkspace();
  api.listExtensions.mockRejectedValueOnce({ code: "read", message: "Workspace unavailable" });
  await act(async () => {
    expect(await result.current.ext.applyMcpEdit("mcp", { expectedRevision: 1, fields: {} }))
      .toEqual({ definition: { id: "mcp", name: "docs", revision: 2 }, deployment: "unverified" });
  });
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("still redeploys an MCP edit whose bindings are disabled", async () => {
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...mcp,
    bindings: [{ ...skill.bindings[0], resourceId: "mcp", desired: "disabled" }],
  }] });
  api.updateMcpDefinition.mockResolvedValue({ id: "mcp", name: "renamed", revision: 2 });
  const { result } = await renderOperationWorkspace();
  await act(async () => {
    expect((await result.current.ext.applyMcpEdit("mcp", { expectedRevision: 1, serverKey: "renamed" }))?.deployment)
      .toBe("required");
  });
});

it("does not claim a Skill file save is fully refreshed when the read fails", async () => {
  api.updateSkillFiles.mockResolvedValue({ id: "skill", name: "rules", revision: 2 });
  const { result } = await renderOperationWorkspace();
  api.listExtensions.mockRejectedValueOnce({ code: "read", message: "Workspace unavailable" });
  await act(async () => {
    expect((await result.current.ext.saveSkillFiles("skill", {
      expectedRevision: 1, expectedDigest: skill.contentDigest, files: [],
    }))?.deployment).toBe("unverified");
  });
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("keeps recovery available and only reports success after recovery is verified", async () => {
  api.listExtensions.mockResolvedValue({ ...workspace, recoveryRequired: ["interrupted"] });
  api.recoverExtensionTransactions.mockResolvedValue(["interrupted: still locked"]);
  const { result } = await renderOperationWorkspace();
  expect(result.current.writeBlocked).toBe(true);
  await act(async () => { await result.current.ext.recoverTransactions(); });
  expect(result.current.writeBlocked).toBe(true);
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
  api.listExtensions.mockResolvedValue(workspace);
  await act(async () => { await result.current.ext.recoverTransactions(); });
  expect(result.current.writeBlocked).toBe(false);
  expect(notify).toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});
