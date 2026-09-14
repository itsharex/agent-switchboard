import { useCallback } from "react";
import {
  deleteExtension,
  getMcpEditView,
  recoverExtensionTransactions,
  registerProject,
  saveExtension,
  setBindingLock,
  updateMcpDefinition,
  putExtensionSecret,
  type ExtensionDraft,
  type McpEditRequest,
  type McpEditViewEnvelope,
  type ProjectRegistration,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type {
  EditPreparation,
  ExclusiveRunner,
  ExtensionReader,
  WorkspaceRefresher,
} from "./extension-ops";
import { definitionUpdatePreparation, refreshLibraryWrite } from "./extension-ops";

interface LibraryDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
  runRead: ExtensionReader;
}

function useDefinitionMutations({ refresh, runExclusive }: LibraryDeps) {
  const saveDefinition = useCallback(
    (draft: ExtensionDraft) =>
      runExclusive(async () => {
        const definition = await saveExtension(draft);
        await refreshLibraryWrite(refresh, { kind: "info", title: "已保存扩展定义" });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const applyMcpEdit = useCallback(
    (definitionId: string, edit: McpEditRequest): Promise<EditPreparation | null> =>
      runExclusive(async () => {
        const definition = await updateMcpDefinition(definitionId, edit);
        return definitionUpdatePreparation(definition, await refresh());
      }),
    [refresh, runExclusive],
  );

  const removeDefinition = useCallback(
    (id: string) =>
      runExclusive(async () => {
        await deleteExtension(id, true);
        await refreshLibraryWrite(refresh, { title: "已删除扩展定义" });
        return true;
      }),
    [refresh, runExclusive],
  );
  return { saveDefinition, applyMcpEdit, removeDefinition };
}

function useExtensionMaintenance({ refresh, runExclusive }: LibraryDeps) {
  const recoverTransactions = useCallback(
    () => runExclusive(async () => {
      const results = await recoverExtensionTransactions();
      const workspace = await refresh();
      toast({
        kind: workspace !== null && workspace.recoveryRequired.length === 0 ? "success" : "warning",
        title: workspace === null
          ? "恢复检查已执行，状态刷新失败"
          : workspace.recoveryRequired.length > 0
            ? "仍有扩展事务需要恢复"
            : "已完成扩展事务恢复检查",
        description: results.join("；") || "没有需要恢复的事务",
      });
      return results;
    }),
    [refresh, runExclusive],
  );
  const toggleBindingLock = useCallback(
    (bindingId: string, locked: boolean) =>
      runExclusive(async () => {
        await setBindingLock(bindingId, locked);
        await refreshLibraryWrite(refresh, {
          title: locked ? "已固定该目标的 Skill 版本" : "已解除版本固定",
          description: locked
            ? "来源或内容更新不再影响该目标，直至解除固定。"
            : "该目标重新跟随扩展库的当前内容版本。",
        });
        return true;
      }),
    [refresh, runExclusive],
  );

  const addProject = useCallback(
    (root: string) =>
      runExclusive(async (): Promise<ProjectRegistration> => {
        const project = await registerProject(root);
        await refreshLibraryWrite(refresh, {
          title: "已注册项目目录",
          description: project.displayName,
        });
        return project;
      }),
    [refresh, runExclusive],
  );
  return { recoverTransactions, toggleBindingLock, addProject };
}

/** Definition writes remain separate from the client's prepare/apply transaction. */
export function useExtensionLibrary(deps: LibraryDeps) {
  const { runRead, runExclusive } = deps;
  const definitions = useDefinitionMutations(deps);
  const maintenance = useExtensionMaintenance(deps);
  const loadMcpEdit = useCallback(
    (definitionId: string) => runRead((): Promise<McpEditViewEnvelope> => getMcpEditView(definitionId)),
    [runRead],
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
    ...definitions,
    ...maintenance,
    loadMcpEdit,
    putSecret,
  };
}
