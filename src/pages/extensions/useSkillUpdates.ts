import { useState } from "react";
import type { ExtensionListItem, SkillUpdateReport } from "../../api/client";
import type { useExtensions } from "../../app/useExtensions";
import { toast } from "../../components/use-toast";
import type { ExtensionApplies } from "./useExtensionApplies";

type Extensions = ReturnType<typeof useExtensions>;

function canUpdate(report: SkillUpdateReport, items: ExtensionListItem[]) {
  if (report.error !== null || report.upToDate || report.newDigest === null) return false;
  const item = items.find((entry) => entry.id === report.definitionId);
  return item?.kind === "skill" && (item.contentDigest !== report.newDigest ||
    item.bindings.some((binding) => binding.desired === "enabled" &&
      !binding.lockedDigest && binding.fileState === "pendingApply"));
}

async function updateSkills(
  ext: Extensions,
  applies: ExtensionApplies,
  available: SkillUpdateReport[],
  selected: SkillUpdateReport[],
  complete: (ids: string[]) => void,
): Promise<boolean> {
  const chosen = [...new Map(selected.filter((report) => available.some((entry) =>
    entry.definitionId === report.definitionId && entry.newDigest === report.newDigest))
    .map((report) => [report.definitionId, report])).values()];
  if (chosen.length === 0) return false;
  const items = ext.workspace?.items ?? [];
  const entries = chosen.flatMap((report) => {
    const item = items.find((entry) => entry.id === report.definitionId);
    return item?.kind === "skill" && item.contentDigest !== report.newDigest && report.newDigest
      ? [{ definitionId: report.definitionId, newDigest: report.newDigest }] : [];
  });
  const result = entries.length > 0 ? await ext.advanceSkillUpdates(entries)
    : { advanced: [], failed: [], workspace: ext.workspace };
  if (!result) return false;
  if (result.failed.length > 0) toast({
    kind: "warning", title: "部分 Skill 更新失败",
    description: result.failed.map((failure) =>
      (items.find((item) => item.id === failure.definitionId)?.name ?? failure.definitionId) +
      "：" + failure.message).join("；"),
  });
  if (result.workspace === null) return false;
  const currentItems = result.workspace.items;
  const ready = chosen.filter((report) => result.advanced.includes(report.definitionId) ||
    currentItems.some((item) => item.id === report.definitionId && item.kind === "skill" &&
      item.contentDigest === report.newDigest)).map((report) => report.definitionId);
  const operations = ready.filter((id) => currentItems.some((item) => item.id === id &&
    item.bindings.some((binding) => binding.desired === "enabled" && !binding.lockedDigest)))
    .map((definitionId) => ({ operation: "update" as const, definitionId }));
  if (operations.length > 0) {
    const applied = await applies.run({ operations });
    if (applied.status !== "applied") {
      complete(ready.filter((id) => !operations.some((operation) => operation.definitionId === id)));
      return false;
    }
  } else if (ready.length > 0) toast({ kind: "success", title: "Skill 已更新到扩展库" });
  complete(ready);
  return ready.length > 0 && result.failed.length === 0;
}

export function useSkillUpdates(ext: Extensions, applies: ExtensionApplies) {
  const [reports, setReports] = useState<SkillUpdateReport[]>([]);
  const items = ext.workspace?.items ?? [];
  const reportMap = new Map(reports.map((report) => [report.definitionId, report]));
  const updatable = reports.filter((report) => canUpdate(report, items));
  const check = async (ids: string[]) => {
    if (ids.length === 0) return;
    const result = await ext.checkUpdates(ids);
    if (!result) return;
    setReports((previous) => [...result, ...previous.filter((report) => !ids.includes(report.definitionId))]);
    const failures = result.filter((report) => report.error !== null);
    if (failures.length > 0) toast({
      kind: "warning", title: failures.length + " 个 Skill 检查更新失败",
      description: failures.map((report) =>
        (items.find((item) => item.id === report.definitionId)?.name ?? report.definitionId) +
        "：" + report.error).join("；"),
    });
    else if (result.length > 0 && result.every((report) => report.upToDate)) {
      toast({ kind: "success", title: "所检查的 Skill 来源内容均为最新" });
    }
  };
  const update = (selected: SkillUpdateReport[]) => updateSkills(ext, applies, updatable, selected,
    (ids) => setReports((previous) => previous.filter((report) => !ids.includes(report.definitionId))));
  return { reportMap, updatable, check, update };
}
