import { useCallback } from "react";
import {
  importDiscoveredMcp, importDiscoveredSkill, takeoverDiscoveredExtension,
  type ObservedExtension,
} from "../../api/client";
import { toast, toastMessage } from "../../components/use-toast";
import type { ExclusiveRunner, WorkspaceRefresher } from "./extension-ops";

export interface DiscoveryImportResult {
  importedIds: string[];
  failed: Array<{ name: string; error: unknown }>;
}

export function discoveryImportMode(item: ObservedExtension): "manage" | "copy" | null {
  if (item.managed) return null;
  const native = item.origin.origin !== "legacyRoot" && item.origin.origin !== "managed";
  if (native && item.actions.takeover.supported) return "manage";
  return item.actions.import.supported && !item.actions.import.inLibrary ? "copy" : null;
}

async function importOne(item: ObservedExtension) {
  if (discoveryImportMode(item) === "manage") return takeoverDiscoveredExtension(item.observationId);
  return item.kind === "skill"
    ? importDiscoveredSkill(item.observationId)
    : importDiscoveredMcp(item.observationId);
}

async function importBatch(items: ObservedExtension[]): Promise<DiscoveryImportResult> {
  const result: DiscoveryImportResult = { importedIds: [], failed: [] };
  for (const item of items) {
    try {
      await importOne(item);
      result.importedIds.push(item.observationId);
    } catch (caught) {
      result.failed.push({ name: item.name, error: caught });
    }
  }
  return result;
}

export function useDiscoveryImport({ refresh, scan, runExclusive }: {
  refresh: WorkspaceRefresher;
  scan: () => Promise<unknown>;
  runExclusive: ExclusiveRunner;
}) {
  return useCallback((items: ObservedExtension[]) => runExclusive(async () => {
    const unique = new Map(items.filter((item) => discoveryImportMode(item))
      .map((item) => [item.observationId, item]));
    const result = await importBatch([...unique.values()]);
    if (result.importedIds.length > 0) {
      const refreshed = await refresh();
      const scanned = await scan();
      toast({
        kind: refreshed && scanned ? "success" : "warning",
        title: refreshed && scanned ? toastMessage("importDiscovery.extToast.imported") : toastMessage("importDiscovery.extToast.partial"),
        description: toastMessage("importDiscovery.extToast.description", { count: result.importedIds.length }),
      });
    }
    return result;
  }), [refresh, scan, runExclusive]);
}
