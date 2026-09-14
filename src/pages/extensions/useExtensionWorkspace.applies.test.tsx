import { act, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";
import type { ExtensionDraft, ExtensionMutation } from "../../api/client";
import { api, deferred, notify, onError, prepared, record, renderOperationWorkspace, skill } from "./extension-apply.test-support";

const draft: ExtensionDraft = { name: "docs", payload: { kind: "mcp", transport: "stdio", command: "npx" } };

it.each(["rejection", "rollback", "apply error", "prepare error"])(
  "never deletes a bound definition following %s", async (failure) => {
    if (failure === "rejection") api.applyExtensionPlan.mockResolvedValue({ record: null, rejected: "Plan stale", rolledBack: false });
    if (failure === "rollback") api.applyExtensionPlan.mockResolvedValue({ record, rejected: null, rolledBack: true });
    if (failure === "apply error") api.applyExtensionPlan.mockRejectedValue({ code: "write", message: "Write failed" });
    if (failure === "prepare error") api.prepareExtensionPlan.mockRejectedValue({ code: "prepare", message: "Prepare failed" });
    const { result } = await renderOperationWorkspace();
    await act(async () => { expect(await result.current.deleteDefinition(skill)).toBe(false); });
    expect(api.deleteExtension).not.toHaveBeenCalled();
    expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
    expect(result.current.busy).toBe(false);
    if (failure.endsWith("error")) expect(onError).toHaveBeenCalled();
  },
);

it("waits for removal confirmation and does not delete after cancellation", async () => {
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const { result } = await renderOperationWorkspace();
  let deletion!: Promise<boolean>;
  act(() => { deletion = result.current.deleteDefinition(skill); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  expect(api.deleteExtension).not.toHaveBeenCalled();
  await act(async () => {
    result.current.applies.cancelPendingWrite();
    expect(await deletion).toBe(false);
  });
  expect(api.deleteExtension).not.toHaveBeenCalled();
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});

it("does not delete after a successful removal whose state could not be refreshed", async () => {
  const { result } = await renderOperationWorkspace();
  api.listExtensions.mockRejectedValueOnce({ code: "read", message: "Workspace unavailable" });
  await act(async () => { expect(await result.current.deleteDefinition(skill)).toBe(false); });
  expect(api.deleteExtension).not.toHaveBeenCalled();
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("deletes only after applying and refreshing the binding removal", async () => {
  const { result } = await renderOperationWorkspace();
  await act(async () => { expect(await result.current.deleteDefinition(skill)).toBe(true); });
  expect(api.deleteExtension).toHaveBeenCalledWith("skill", true);
  expect(api.applyExtensionPlan.mock.invocationCallOrder[0]).toBeLessThan(api.deleteExtension.mock.invocationCallOrder[0]);
  expect(api.listExtensions.mock.invocationCallOrder[1]).toBeLessThan(api.deleteExtension.mock.invocationCallOrder[0]);
});

it("creates a definition and deploys normally across an actual busy render", async () => {
  const saved = deferred<ExtensionMutation>();
  api.saveExtension.mockReturnValue(saved.promise);
  const { result } = await renderOperationWorkspace();
  let creation!: Promise<boolean>;
  act(() => { creation = result.current.createMcp(draft, ["codex", "claude"]); });
  await waitFor(() => expect(result.current.busy).toBe(true));
  await act(async () => {
    saved.resolve({ id: "mcp", name: "docs", revision: 1 });
    expect(await creation).toBe(true);
  });
  expect(api.prepareExtensionPlan).toHaveBeenCalledWith({ operations: [{
    operation: "install", definitionId: "mcp", targets: [
      { scope: "app", client: "codex" }, { scope: "app", client: "claude" },
    ],
  }] });
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
  expect(result.current.busy).toBe(false);
});

it("does not resave or report success when new MCP deployment is cancelled", async () => {
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const { result } = await renderOperationWorkspace();
  let creation!: Promise<boolean>;
  act(() => { creation = result.current.createMcp(draft, ["codex"]); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  await act(async () => {
    expect(await result.current.createMcp(draft, ["codex"])).toBe(false);
    result.current.applies.cancelPendingWrite();
    expect(await creation).toBe(false);
  });
  expect(api.saveExtension).toHaveBeenCalledTimes(1);
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
  expect(result.current.nav.dialog).toEqual({ type: "detail", definitionId: "mcp" });
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});
