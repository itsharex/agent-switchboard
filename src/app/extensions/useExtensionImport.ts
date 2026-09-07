import { useCallback } from "react";
import {
  discoverExtensions,
  exportExtensionPortable,
  importDiscoveredMcp,
  importDiscoveredSkill,
  importExtensionPortable,
  importSkillCandidate,
  previewDiscoveredTakeover,
  resolveSkillSource,
  scanLocalSkillSource,
  takeoverDiscoveredExtension,
  type AppKind,
  type ExtensionMutation,
  type PortableImportReport,
  type SkillCandidateDto,
  type TakeoverPreview,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type { ExclusiveRunner, WorkspaceRefresher } from "./extension-ops";

interface ImportDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
}

/** Discovery, source scanning, imports, takeovers, and portable packages. */
export function useExtensionImport({ refresh, runExclusive }: ImportDeps) {
  const discover = useCallback(
    () => runExclusive(() => discoverExtensions()),
    [runExclusive],
  );

  const scanLocal = useCallback(
    (root: string) => runExclusive((): Promise<SkillCandidateDto[]> => scanLocalSkillSource(root)),
    [runExclusive],
  );

  const resolveSource = useCallback(
    (repo: string, subpath: string, refName: string | null) =>
      runExclusive((): Promise<SkillCandidateDto[]> => resolveSkillSource(repo, subpath, refName)),
    [runExclusive],
  );

  const importCandidate = useCallback(
    (digest: string, name: string, hostScoped: AppKind | null) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await importSkillCandidate(digest, name, hostScoped);
        await refresh();
        toast({ kind: "success", title: "已导入 Skill 到扩展库", description: definition.name });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const importObservedSkill = useCallback(
    (observationId: string) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await importDiscoveredSkill(observationId);
        await refresh();
        toast({ kind: "success", title: "已导入 Skill 到扩展库", description: definition.name });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const importObservedMcp = useCallback(
    (observationId: string) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await importDiscoveredMcp(observationId);
        await refresh();
        toast({
          kind: "success",
          title: "已将 MCP 加入扩展库",
          description: "本机现有配置保持只读；请在详情中预览后再部署。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  /** Loads the redacted takeover preview; the caller renders it for
   * confirmation before the takeover itself is requested. */
  const previewTakeover = useCallback(
    (observationId: string) =>
      runExclusive((): Promise<TakeoverPreview> => previewDiscoveredTakeover(observationId)),
    [runExclusive],
  );

  const takeoverObserved = useCallback(
    (observationId: string) =>
      runExclusive(async (): Promise<ExtensionMutation> => {
        const definition = await takeoverDiscoveredExtension(observationId);
        await refresh();
        toast({
          kind: "success",
          title: "已接管本机扩展",
          description: "客户端文件保持原样；移除绑定时将恢复接管时的原始内容。",
        });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const exportPortable = useCallback(
    (definitionId: string, targetPath: string) =>
      runExclusive(async (): Promise<boolean> => {
        await exportExtensionPortable(definitionId, targetPath);
        toast({ kind: "success", title: "已导出便携包", description: targetPath });
        return true;
      }),
    [runExclusive],
  );

  const importPortable = useCallback(
    (packagePath: string) =>
      runExclusive(async (): Promise<PortableImportReport> => {
        const report = await importExtensionPortable(packagePath);
        await refresh();
        const missing = report.missingEnvSlots;
        toast({
          kind: missing.length > 0 ? "warning" : "success",
          title: "已导入便携包",
          description:
            missing.length > 0
              ? `请在编辑器中补配置这些凭据槽：${missing.join("、")}`
              : report.warnings[0] ?? "定义已加入扩展库",
        });
        return report;
      }),
    [refresh, runExclusive],
  );

  return {
    discover,
    scanLocal,
    resolveSource,
    importCandidate,
    importObservedSkill,
    importObservedMcp,
    previewTakeover,
    takeoverObserved,
    exportPortable,
    importPortable,
  };
}
