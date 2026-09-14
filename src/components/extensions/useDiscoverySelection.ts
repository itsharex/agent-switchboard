import { useEffect, useState } from "react";
import type { ObservedExtension } from "../../api/client";
import { discoveryImportMode, type DiscoveryImportResult } from "../../app/extensions/useDiscoveryImport";

export function useDiscoverySelection(rows: ObservedExtension[], scanId: string | undefined, kind: string) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [result, setResult] = useState<DiscoveryImportResult | null>(null);
  const eligible = rows.filter((item) => discoveryImportMode(item) !== null);
  useEffect(() => {
    setSelected(new Set(eligible.map((item) => item.observationId)));
    // A new scan invalidates every observation id, including failed import ids.
  }, [scanId, kind]);
  const select = (id: string, checked: boolean) => setSelected((current) => {
    const next = new Set(current);
    if (checked) next.add(id); else next.delete(id);
    return next;
  });
  const selectAll = (visible: ObservedExtension[], checked: boolean) => setSelected((current) => {
    const next = new Set(current);
    for (const item of visible) {
      if (discoveryImportMode(item) === null) continue;
      if (checked) next.add(item.observationId); else next.delete(item.observationId);
    }
    return next;
  });
  return { selected, select, selectAll, result, setResult,
    selectedItems: eligible.filter((item) => selected.has(item.observationId)) };
}
