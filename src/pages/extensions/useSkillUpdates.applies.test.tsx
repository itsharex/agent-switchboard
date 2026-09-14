import { act, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";
import {
  api, notify, prepared, renderOperationWorkspace, skill, updateReport, workspace,
} from "./extension-apply.test-support";

it("does not deploy or report success for a failed content update", async () => {
  api.checkSkillUpdates.mockResolvedValue([updateReport]);
  api.updateSkillDefinition.mockRejectedValueOnce({ code: "source", message: "Candidate expired" });
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.updates.check(["skill"]); });
  await act(async () => { expect(await result.current.updates.update([updateReport])).toBe(false); });
  expect(api.prepareExtensionPlan).not.toHaveBeenCalled();
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
  expect(result.current.updates.updatable).toHaveLength(1);
});

it("keeps a cancelled deployment retryable without advancing the saved content again", async () => {
  api.checkSkillUpdates.mockResolvedValue([updateReport]);
  api.prepareExtensionPlan.mockResolvedValue(prepared(true));
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.updates.check(["skill"]); });
  const updatedSkill = { ...skill, revision: 2, contentDigest: updateReport.newDigest!,
    bindings: [{ ...skill.bindings[0], fileState: "pendingApply" as const }] };
  api.listExtensions.mockResolvedValue({ ...workspace, items: [updatedSkill] });
  let update!: Promise<boolean>;
  act(() => { update = result.current.updates.update([updateReport]); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  await act(async () => {
    result.current.applies.cancelPendingWrite();
    expect(await update).toBe(false);
  });
  expect(result.current.updates.updatable).toHaveLength(1);
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
  act(() => { update = result.current.updates.update(result.current.updates.updatable); });
  await waitFor(() => expect(result.current.applies.pendingWrite).not.toBeNull());
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...updatedSkill,
    bindings: [{ ...updatedSkill.bindings[0], fileState: "inSync", lastAppliedRevision: 2 }],
  }] });
  await act(async () => {
    result.current.applies.confirmPendingWrite();
    expect(await update).toBe(true);
  });
  expect(api.updateSkillDefinition).toHaveBeenCalledTimes(1);
  expect(api.applyExtensionPlan).toHaveBeenCalledTimes(1);
  expect(result.current.updates.reportMap.size).toBe(0);
});

it("does not lose a pending update when the post-save workspace refresh fails", async () => {
  api.checkSkillUpdates.mockResolvedValue([updateReport]);
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.updates.check(["skill"]); });
  api.listExtensions.mockRejectedValueOnce({ code: "read", message: "Workspace unavailable" });
  await act(async () => { expect(await result.current.updates.update([updateReport])).toBe(false); });
  expect(result.current.updates.updatable).toHaveLength(1);
  expect(api.prepareExtensionPlan).not.toHaveBeenCalled();
  expect(notify).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "success" }));
});

it("uses refreshed bindings to decide deployment instead of the pre-save render", async () => {
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...skill,
    bindings: [{ ...skill.bindings[0], desired: "disabled" }],
  }] });
  api.checkSkillUpdates.mockResolvedValue([updateReport]);
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.updates.check(["skill"]); });
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...skill,
    revision: 2, contentDigest: updateReport.newDigest!,
  }] });
  await act(async () => { expect(await result.current.updates.update([updateReport])).toBe(true); });
  expect(api.prepareExtensionPlan).toHaveBeenCalledWith({
    operations: [{ operation: "update", definitionId: "skill" }],
  });
});

it("keeps a pinned Skill binding unchanged when only its library content advances", async () => {
  const locked = { ...skill, bindings: [{ ...skill.bindings[0], lockedDigest: skill.contentDigest }] };
  api.listExtensions.mockResolvedValue({ ...workspace, items: [locked] });
  api.checkSkillUpdates.mockResolvedValue([updateReport]);
  const { result } = await renderOperationWorkspace();
  await act(async () => { await result.current.updates.check(["skill"]); });
  api.listExtensions.mockResolvedValue({ ...workspace, items: [{ ...locked,
    revision: 2, contentDigest: updateReport.newDigest!,
  }] });
  await act(async () => { expect(await result.current.updates.update([updateReport])).toBe(true); });
  expect(api.updateSkillDefinition).toHaveBeenCalledTimes(1);
  expect(api.prepareExtensionPlan).not.toHaveBeenCalled();
  expect(api.applyExtensionPlan).not.toHaveBeenCalled();
});
