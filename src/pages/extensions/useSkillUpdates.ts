import { useState } from "react";
import type { SkillUpdateReport } from "../../api/client";
import type { useExtensions } from "../../app/useExtensions";
import { toast } from "../../components/use-toast";
import type { useExtensionPlans } from "./useExtensionPlans";

export function useSkillUpdates(
  ext: ReturnType<typeof useExtensions>,
  plans: ReturnType<typeof useExtensionPlans>,
) {
  const [reports, setReports] = useState<SkillUpdateReport[]>([]);
  const items = ext.workspace?.items ?? [];
  const reportMap = new Map(reports.map((report) => [report.definitionId, report]));
  const updatable = reports.filter(
    (report) =>
      report.error === null &&
      !report.upToDate &&
      report.newDigest !== null &&
      items.some(
        (item) =>
          item.id === report.definitionId && item.kind === "skill" && item.contentDigest !== report.newDigest,
      ),
  );
  const check = async (ids: string[]) => {
    if (ids.length === 0) return;
    const result = await ext.checkUpdates(ids);
    if (!result) return;
    setReports((previous) => [...result, ...previous.filter((report) => !ids.includes(report.definitionId))]);
    const failures = result.filter((report) => report.error !== null);
    if (failures.length > 0) {
      toast({
        kind: "warning",
        title: `${failures.length} 个 Skill 检查更新失败`,
        description: failures
          .map((report) => `${items.find((item) => item.id === report.definitionId)?.name}：${report.error}`)
          .join("；"),
      });
    } else if (result.every((report) => report.upToDate)) {
      toast({ kind: "success", title: "所检查的 Skill 来源内容均为最新" });
    }
  };
  const update = async (selected: SkillUpdateReport[]) => {
    const entries = selected.flatMap((report) =>
      updatable.includes(report) && report.newDigest
        ? [{ definitionId: report.definitionId, newDigest: report.newDigest }]
        : [],
    );
    if (entries.length === 0) return;
    const result = await ext.advanceSkillUpdates(entries);
    if (!result) return;
    setReports((previous) => previous.filter((report) => !result.advanced.includes(report.definitionId)));
    if (result.failed.length > 0) {
      toast({
        kind: "warning",
        title: "部分 Skill 更新失败",
        description: result.failed
          .map(
            (failure) =>
              `${items.find((item) => item.id === failure.definitionId)?.name}：${failure.message}`,
          )
          .join("；"),
      });
    }
    const operations = result.advanced
      .filter((id) =>
        items.some(
          (item) => item.id === id && item.bindings.some((binding) => binding.desired === "enabled"),
        ),
      )
      .map((definitionId) => ({ operation: "update" as const, definitionId }));
    if (operations.length > 0) await plans.prepare({ operations });
    else if (result.advanced.length > 0) toast({ kind: "success", title: "Skill 已更新到扩展库" });
  };
  return { reportMap, updatable, check, update };
}
