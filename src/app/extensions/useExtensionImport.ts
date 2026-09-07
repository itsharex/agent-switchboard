import { useCallback } from "react";
import {
  exportExtensionPortable,
  importExtensionPortable,
  importSkillCandidate,
  resolveSkillSource,
  scanLocalSkillSource,
  type AppKind,
  type ExtensionMutation,
  type PortableImportReport,
  type SkillCandidateDto,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type { ExclusiveRunner, WorkspaceRefresher } from "./extension-ops";

interface ImportDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
}

/** Source scanning, imports, and portable packages. Discovery-scan reads,
 * imports from discovered rows, and takeovers live in useDiscoverScan. */
export function useExtensionImport({ refresh, runExclusive }: ImportDeps) {
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
    scanLocal,
    resolveSource,
    importCandidate,
    exportPortable,
    importPortable,
  };
}
