import { useCallback } from "react";
import {
  checkSkillUpdates,
  createLocalSkill,
  forkLocalSkill,
  getSkillEditor,
  listSkillVersions,
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
  ExtensionReader,
  SkillBatchAdvance,
  SkillUpdatePreparation,
  WorkspaceRefresher,
} from "./extension-ops";
import { definitionUpdatePreparation, refreshLibraryWrite } from "./extension-ops";

interface SkillLibraryDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
  runRead: ExtensionReader;
}

function useSkillSourceUpdates({ refresh, runExclusive }: SkillLibraryDeps) {
  const checkUpdates = useCallback(
    (definitionIds: string[]) =>
      runExclusive((): Promise<SkillUpdateReport[]> => checkSkillUpdates(definitionIds)),
    [runExclusive],
  );

  /** Advances many skill definitions to their freshly checked versions in
   * one exclusive run. Per-item failures are reported, not thrown, so one
   * expired candidate never blocks the rest of the batch; the caller deploys
   * the advanced ids through the shared apply pipeline. */
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
        const workspace = await refresh();
        if (workspace === null) toast({
          kind: "warning",
          title: "更新结果尚未验证",
          description: "扩展状态刷新失败；请刷新后确认内容版本和客户端部署。",
        });
        return { advanced, failed, workspace };
      }),
    [refresh, runExclusive],
  );

  const applySkillUpdate = useCallback(
    (definitionId: string, newDigest: string) =>
      runExclusive(async (): Promise<SkillUpdatePreparation> => {
        const definition = await updateSkillDefinition(definitionId, newDigest);
        return definitionUpdatePreparation(definition, await refresh());
      }),
    [refresh, runExclusive],
  );
  return { checkUpdates, advanceSkillUpdates, applySkillUpdate, restoreSkillVersion: applySkillUpdate };
}

function useLocalSkills({ refresh, runExclusive }: SkillLibraryDeps) {
  const createSkill = useCallback(
    (draft: LocalSkillDraft) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await createLocalSkill(draft);
        await refreshLibraryWrite(refresh, {
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
        await refreshLibraryWrite(refresh, {
          title: "已创建本地副本",
          description: "副本不再跟随来源更新，可在编辑器中修改。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );
  return { createSkill, forkSkill };
}

function useSkillContent({ refresh, runExclusive, runRead }: SkillLibraryDeps) {
  const loadSkillEditor = useCallback(
    (definitionId: string) =>
      runRead((): Promise<SkillEditorView> => getSkillEditor(definitionId)),
    [runRead],
  );

  /** Publishes one edited content version. Whenever enabled bindings exist,
   * the caller immediately deploys the update through the shared apply
   * pipeline. */
  const saveSkillFiles = useCallback(
    (definitionId: string, update: SkillFilesUpdate): Promise<SkillUpdatePreparation | null> =>
      runExclusive(async () => {
        const definition = await updateSkillFiles(definitionId, update);
        return definitionUpdatePreparation(definition, await refresh());
      }),
    [refresh, runExclusive],
  );

  const loadSkillVersions = useCallback(
    (definitionId: string) =>
      runRead((): Promise<SkillVersion[]> => listSkillVersions(definitionId)),
    [runRead],
  );

  const saveSkillDependencies = useCallback(
    (definitionId: string, update: SkillDependenciesUpdate) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await updateSkillDependencies(definitionId, update);
        await refreshLibraryWrite(refresh, {
          title: "已更新依赖关联",
          description: "下次部署时会一并部署依赖的 MCP。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  return {
    loadSkillEditor,
    saveSkillFiles,
    loadSkillVersions,
    saveSkillDependencies,
  };
}

export function useSkillLibrary(deps: SkillLibraryDeps) {
  const sources = useSkillSourceUpdates(deps);
  const local = useLocalSkills(deps);
  const content = useSkillContent(deps);
  return { ...sources, ...local, ...content };
}
