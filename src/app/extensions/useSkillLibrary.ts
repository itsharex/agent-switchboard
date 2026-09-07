import { useCallback } from "react";
import {
  checkSkillUpdates,
  createLocalSkill,
  forkLocalSkill,
  getSkillEditor,
  listSkillVersions,
  prepareExtensionPlan,
  updateSkillDefinition,
  updateSkillDependencies,
  updateSkillFiles,
  type ExtensionMutation,
  type LocalSkillDraft,
  type SkillDependenciesUpdate,
  type SkillEditorView,
  type SkillFilesUpdate,
  type SkillUpdateReport,
  type SkillVersion,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type {
  ExclusiveRunner,
  SkillBatchAdvance,
  SkillUpdatePreparation,
  WorkspaceRefresher,
} from "./extension-ops";

interface SkillLibraryDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
}

/** Skill-content operations: creation, forking, file versions, and
 * dependency links; every enabled-binding write keeps the plan preview. */
export function useSkillLibrary({ refresh, runExclusive }: SkillLibraryDeps) {
  const checkUpdates = useCallback(
    (definitionIds: string[]) =>
      runExclusive((): Promise<SkillUpdateReport[]> => checkSkillUpdates(definitionIds)),
    [runExclusive],
  );

  /** Advances many skill definitions to their freshly checked versions in
   * one exclusive run. Per-item failures are reported, not thrown, so one
   * expired candidate never blocks the rest of the batch; the combined
   * deployment preview is the caller's next step. */
  const advanceSkillUpdates = useCallback(
    (entries: Array<{ definitionId: string; newDigest: string }>) =>
      runExclusive(async (): Promise<SkillBatchAdvance> => {
        const advanced: string[] = [];
        const failed: Array<{ definitionId: string; message: string }> = [];
        for (const entry of entries) {
          try {
            await updateSkillDefinition(entry.definitionId, entry.newDigest);
            advanced.push(entry.definitionId);
          } catch (caught) {
            failed.push({
              definitionId: entry.definitionId,
              message: (caught as { message?: string }).message ?? "内容更新失败",
            });
          }
        }
        await refresh();
        return { advanced, failed };
      }),
    [refresh, runExclusive],
  );

  const applySkillUpdate = useCallback(
    (definitionId: string, newDigest: string) =>
      runExclusive(async (): Promise<SkillUpdatePreparation> => {
        const definition = await updateSkillDefinition(definitionId, newDigest);
        const workspace = await refresh();
        if (workspace === null) {
          toast({
            kind: "warning",
            title: "内容版本已入库，但无法读取部署状态",
            description: "刷新扩展列表后，使用“预览部署当前版本”确认客户端更新。",
          });
          return { definition, plan: null };
        }
        const shouldDeploy = workspace?.items
          .find((item) => item.id === definition.id)
          ?.bindings.some((binding) => binding.desired === "enabled") ?? false;
        const plan = shouldDeploy
          ? await prepareExtensionPlan({ operations: [{ operation: "update", definitionId: definition.id }] })
          : null;
        toast(
          plan
            ? { kind: "success", title: "内容版本已入库；请确认部署预览" }
            : { kind: "success", title: "内容版本已入库；没有启用的绑定需要部署" },
        );
        return { definition, plan };
      }),
    [refresh, runExclusive],
  );

  const createSkill = useCallback(
    (draft: LocalSkillDraft) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await createLocalSkill(draft);
        await refresh();
        toast({
          kind: "success",
          title: "已创建本地 Skill",
          description: "模板内容已作为首个不可变版本入库，可继续编辑。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const forkSkill = useCallback(
    (definitionId: string) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await forkLocalSkill(definitionId);
        await refresh();
        toast({
          kind: "success",
          title: "已创建本地副本",
          description: "副本不再跟随来源更新，可在编辑器中修改。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const loadSkillEditor = useCallback(
    (definitionId: string) =>
      runExclusive((): Promise<SkillEditorView> => getSkillEditor(definitionId)),
    [runExclusive],
  );

  /** Publishes one edited content version. Whenever enabled bindings exist,
   * the client update goes through the same plan preview as every other
   * extension write. */
  const saveSkillFiles = useCallback(
    (definitionId: string, update: SkillFilesUpdate): Promise<SkillUpdatePreparation | null> =>
      runExclusive(async () => {
        const definition = await updateSkillFiles(definitionId, update);
        const workspace = await refresh();
        if (workspace === null) {
          toast({
            kind: "warning",
            title: "内容版本已入库，但无法读取部署状态",
            description: "刷新扩展列表后，使用“预览部署当前版本”确认客户端更新。",
          });
          return { definition, plan: null };
        }
        const shouldDeploy = workspace?.items
          .find((item) => item.id === definition.id)
          ?.bindings.some((binding) => binding.desired === "enabled") ?? false;
        const plan = shouldDeploy
          ? await prepareExtensionPlan({ operations: [{ operation: "update", definitionId: definition.id }] })
          : null;
        toast(
          plan
            ? { kind: "success", title: "新内容版本已入库；请确认部署预览" }
            : { kind: "success", title: "新内容版本已入库；没有启用的绑定需要部署" },
        );
        return { definition, plan };
      }),
    [refresh, runExclusive],
  );

  const loadSkillVersions = useCallback(
    (definitionId: string) =>
      runExclusive((): Promise<SkillVersion[]> => listSkillVersions(definitionId)),
    [runExclusive],
  );

  /** Rolls the definition back to a stored content version; deploy previews
   * behave exactly like a fresh source update. */
  const restoreSkillVersion = applySkillUpdate;

  const saveSkillDependencies = useCallback(
    (definitionId: string, update: SkillDependenciesUpdate) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await updateSkillDependencies(definitionId, update);
        await refresh();
        toast({
          kind: "success",
          title: "已更新依赖关联",
          description: "部署时可在同一预览中选择是否一并部署依赖的 MCP。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  return {
    checkUpdates,
    advanceSkillUpdates,
    applySkillUpdate,
    createSkill,
    forkSkill,
    loadSkillEditor,
    saveSkillFiles,
    loadSkillVersions,
    restoreSkillVersion,
    saveSkillDependencies,
  };
}
