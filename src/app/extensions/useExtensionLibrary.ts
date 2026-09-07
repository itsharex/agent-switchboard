import { useCallback } from "react";
import {
  applyExtensionPlan,
  deleteExtension,
  getMcpEditView,
  prepareExtensionPlan,
  prepareExtensionRestore,
  putExtensionSecret,
  recoverExtensionTransactions,
  registerProject,
  saveExtension,
  setBindingLock,
  updateMcpDefinition,
  type ExtensionDraft,
  type McpEditRequest,
  type McpEditViewEnvelope,
  type ApplyOutcome,
  type PlanRequest,
  type ProjectRegistration,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type {
  EditPreparation,
  ExclusiveRunner,
  WorkspaceRefresher,
} from "./extension-ops";

interface LibraryDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
}

/** Library-level mutations: definitions, plan preview/apply/restore,
 * projects, and the secret store — all inside the operation frame. */
export function useExtensionLibrary({ refresh, runExclusive }: LibraryDeps) {
  const recoverTransactions = useCallback(
    () =>
      runExclusive(async (): Promise<string[]> => {
        const results = await recoverExtensionTransactions();
        await refresh();
        toast({
          kind: "success",
          title: "已完成扩展事务恢复检查",
          description: results.join("；") || "没有需要恢复的事务",
        });
        return results;
      }),
    [refresh, runExclusive],
  );

  const saveDefinition = useCallback(
    (draft: ExtensionDraft) =>
      runExclusive(async () => {
        const definition = await saveExtension(draft);
        await refresh();
        toast({ kind: "success", title: "已保存扩展定义" });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const loadMcpEdit = useCallback(
    (definitionId: string) =>
      runExclusive((): Promise<McpEditViewEnvelope> => getMcpEditView(definitionId)),
    [runExclusive],
  );

  /** Applies one field-level MCP edit. Kept positions survive inside the
   * backend; whenever bindings exist — enabled or disabled — the client
   * update goes through the same plan preview as every other extension
   * write (a renamed key moves disabled bindings' traces too). */
  const applyMcpEdit = useCallback(
    (definitionId: string, edit: McpEditRequest): Promise<EditPreparation | null> =>
      runExclusive(async () => {
        const definition = await updateMcpDefinition(definitionId, edit);
        const workspace = await refresh();
        if (workspace === null) {
          toast({
            kind: "warning",
            title: "扩展定义已保存，但无法读取部署状态",
            description: "刷新扩展列表后，使用“预览部署当前版本”确认客户端更新。",
          });
          return { definition, plan: null };
        }
        const shouldDeploy =
          (workspace?.items.find((item) => item.id === definition.id)?.bindings.length ?? 0) > 0;
        const plan = shouldDeploy
          ? await prepareExtensionPlan({ operations: [{ operation: "update", definitionId: definition.id }] })
          : null;
        toast(
          plan
            ? { kind: "success", title: "扩展定义已保存；请确认部署预览" }
            : { kind: "success", title: "扩展定义已保存；没有绑定需要部署" },
        );
        return { definition, plan };
      }),
    [refresh, runExclusive],
  );

  const removeDefinition = useCallback(
    (id: string) =>
      runExclusive(async () => {
        await deleteExtension(id, true);
        await refresh();
        toast({ kind: "success", title: "已删除扩展定义" });
        return true;
      }),
    [refresh, runExclusive],
  );

  /** Pins or unpins one Skill binding's content version; a library-only
   * record change, so it never opens a plan preview. */
  const toggleBindingLock = useCallback(
    (bindingId: string, locked: boolean) =>
      runExclusive(async () => {
        await setBindingLock(bindingId, locked);
        await refresh();
        toast({
          kind: "success",
          title: locked ? "已固定该目标的 Skill 版本" : "已解除版本固定",
          description: locked
            ? "来源或内容更新不再影响该目标，直至解除固定。"
            : "该目标重新跟随扩展库的当前内容版本。",
        });
        return true;
      }),
    [refresh, runExclusive],
  );

  const preparePlan = useCallback(
    (request: PlanRequest) => runExclusive(() => prepareExtensionPlan(request)),
    [runExclusive],
  );

  const applyPlan = useCallback(
    (planId: string) =>
      runExclusive(async (): Promise<ApplyOutcome> => {
        const outcome = await applyExtensionPlan(planId, true);
        if (outcome.rejected !== null) {
          toast({ kind: "warning", title: "计划未能应用", description: outcome.rejected });
          return outcome;
        }
        await refresh();
        if (outcome.rolledBack) {
          toast({ kind: "error", title: "应用失败，已回滚全部变更" });
        } else {
          toast({ kind: "success", title: "已应用扩展变更" });
        }
        return outcome;
      }),
    [refresh, runExclusive],
  );

  const prepareRestore = useCallback(
    (operationId: string) => runExclusive(() => prepareExtensionRestore(operationId)),
    [runExclusive],
  );

  const addProject = useCallback(
    (root: string) =>
      runExclusive(async (): Promise<ProjectRegistration> => {
        const project = await registerProject(root);
        await refresh();
        toast({
          kind: "success",
          title: "已注册项目目录",
          description: project.displayName,
        });
        return project;
      }),
    [refresh, runExclusive],
  );

  const putSecret = useCallback(
    (value: string, purpose: string) =>
      runExclusive(async (): Promise<string> => {
        const { reference } = await putExtensionSecret(value, purpose);
        return reference;
      }),
    [runExclusive],
  );

  return {
    recoverTransactions,
    saveDefinition,
    loadMcpEdit,
    applyMcpEdit,
    removeDefinition,
    toggleBindingLock,
    preparePlan,
    applyPlan,
    prepareRestore,
    addProject,
    putSecret,
  };
}
