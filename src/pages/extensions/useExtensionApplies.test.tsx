import { act, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";
import type { ExtensionPlanView, PlanRequest } from "../../api/client";
import type { ExtensionApplyResult } from "./useExtensionApplies";
import {
  api, deferred, notify, onError, prepared, record, renderOperationWorkspace, skill, workspace,
} from "./extension-apply.test-support";

const request: PlanRequest = { operations: [{ operation: "update", definitionId: "skill" }] };

it("holds one operation through sensitive confirmation and returns cancellation", async () => {
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const { result } = await renderOperationWorkspace();
  let pending!: Promise<ExtensionApplyResult>;
  let settled = false;
  act(() => { pending = result.current.applies.run(request); });
  void pending.then(() => { settled = true; });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());

  expect(settled).toBe(false);
  expect(result.current.applies.pendingOperations).toEqual(request.operations);
  expect(result.current.writeBlocked).toBe(true);
  expect(result.current.applies.confirmationBusy).toBe(false);
  await act(async () => {
    expect(await result.current.applies.run(request)).toEqual({ status: "blocked" });
    expect(await result.current.ext.removeDefinition("skill")).toBeNull();
  });
  expect(api.deleteExtension).not.toHaveBeenCalled();
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
  await act(async () => {
    result.current.applies.cancelPendingWrite();
    expect(await pending).toEqual({ status: "cancelled" });
  });
  expect(result.current.applies.pendingOperations).toEqual([]);
  expect(result.current.busy).toBe(false);
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("accepts a local Claude disable scope without resolving the operation early", async () => {
  const sharedSkill = { ...skill, bindings: [{ ...skill.bindings[0],
    target: { scope: "projectShared" as const, client: "claude" as const, projectId: "project" } }] };
  api.listExtensions.mockResolvedValue({ ...workspace, items: [sharedSkill] });
  const { result } = await renderOperationWorkspace();
  const disable: PlanRequest = { operations: [{ operation: "disable", bindingId: "binding" }] };
  let pending!: Promise<ExtensionApplyResult>;
  act(() => { pending = result.current.applies.run(disable); });
  await waitFor(() => expect(result.current.applies.phase).toBe("scope"));
  expect(result.current.applies.pendingOperations).toEqual(disable.operations);
  act(() => result.current.applies.confirmDisableScope());
  expect(api.prepareExtensionPlan).not.toHaveBeenCalled();
  await act(async () => {
    result.current.applies.setSharedSettings(false);
    result.current.applies.confirmDisableScope();
    result.current.applies.confirmDisableScope();
    expect((await pending).status).toBe("applied");
  });
  expect(api.prepareExtensionPlan).toHaveBeenCalledWith({
    operations: [{ operation: "disable", bindingId: "binding", sharedSettings: false }],
  });
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
});

it("cancels a scope question without preparing or applying anything", async () => {
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...skill,
    bindings: [{ ...skill.bindings[0], target: {
      scope: "projectShared", client: "claude", projectId: "project",
    } }],
  }] });
  const { result } = await renderOperationWorkspace();
  let pending!: Promise<ExtensionApplyResult>;
  act(() => { pending = result.current.applies.run({ operations: [{ operation: "disable", bindingId: "binding" }] }); });
  await waitFor(() => expect(result.current.applies.pendingDisable).not.toBeNull());
  await act(async () => {
    result.current.applies.cancelDisableScope();
    expect((await pending).status).toBe("cancelled");
  });
  expect(api.prepareExtensionPlan).not.toHaveBeenCalled();
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});

it("prevents same-frame duplicate prepares and applies", async () => {
  const { result } = await renderOperationWorkspace();
  await act(async () => {
    const first = result.current.applies.run(request);
    const second = result.current.applies.run(request);
    expect(await second).toEqual({ status: "blocked" });
    expect((await first).status).toBe("applied");
  });
  expect(api.prepareExtensionPlan).toHaveBeenCalledTimes(1);
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
});

it("settles pending confirmation when the workspace unmounts", async () => {
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const view = await renderOperationWorkspace();
  let pending!: Promise<ExtensionApplyResult>;
  act(() => { pending = view.result.current.applies.run(request); });
  await waitFor(() => expect(view.result.current.applies.pendingWrite).not.toBeNull());
  view.unmount();
  expect(await pending).toEqual({ status: "cancelled" });
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});

it("does not start a confirmation or write after unmounting during prepare", async () => {
  const plan = deferred<ExtensionPlanView>();
  api.prepareExtensionPlan.mockReturnValue(plan.promise);
  const view = await renderOperationWorkspace();
  let pending!: Promise<ExtensionApplyResult>;
  act(() => { pending = view.result.current.applies.run(request); });
  view.unmount();
  plan.resolve(prepared(true));
  expect(await pending).toEqual({ status: "cancelled" });
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});

it("restores through the same confirmation and can retry a cancelled restore", async () => {
  api.prepareExtensionRestore.mockResolvedValue(prepared(true));
  const { result } = await renderOperationWorkspace();
  let pending!: Promise<ExtensionApplyResult>;
  act(() => { pending = result.current.applies.restore("operation"); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  await act(async () => {
    result.current.applies.cancelPendingWrite();
    expect((await pending).status).toBe("cancelled");
  });
  act(() => { pending = result.current.applies.restore("operation"); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  await act(async () => {
    result.current.applies.confirmPendingWrite();
    result.current.applies.confirmPendingWrite();
    expect((await pending).status).toBe("applied");
  });
  expect(api.prepareExtensionRestore).toHaveBeenCalledTimes(2);
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
});

it("repairs with a single write frame and verifies it by rescanning", async () => {
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.discovery.scan(); });
  await act(async () => {
    expect((await result.current.applies.repair(["diagnostic"])).status).toBe("applied");
  });
  expect(api.prepareExtensionRepair).toHaveBeenCalledWith("scan", ["diagnostic"]);
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
  expect(api.discoverExtensions).toHaveBeenCalledTimes(2);
  expect(result.current.busy).toBe(false);
});

it("does not report a repair as verified if the scan fails", async () => {
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.discovery.scan(); });
  api.discoverExtensions.mockRejectedValueOnce({ code: "scan", message: "Scan unavailable" });
  await act(async () => {
    expect(await result.current.applies.repair(["diagnostic"])).toEqual({ status: "unverified", record });
  });
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
  expect(onError).toHaveBeenCalledWith({ code: "scan", message: "Scan unavailable" });
});

it("keeps a committed write distinct from a failed workspace refresh", async () => {
  const { result } = await renderOperationWorkspace();
  api.listExtensions.mockRejectedValueOnce({ code: "read", message: "Workspace unavailable" });
  await act(async () => {
    expect(await result.current.applies.run(request)).toEqual({ status: "unverified", record });
  });
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
  expect(result.current.busy).toBe(false);
});
