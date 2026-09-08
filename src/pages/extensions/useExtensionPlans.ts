import { useState } from "react";
import type { ExtensionPlanView, PlanRequest } from "../../api/client";
import { needsDisableScope } from "../../app/extensions/deployment-state";
import type { DiscoverScan } from "../../app/extensions/useDiscoverScan";
import type { useExtensions } from "../../app/useExtensions";

export function useExtensionPlans(ext: ReturnType<typeof useExtensions>, discovery: DiscoverScan) {
  const [view, setView] = useState<ExtensionPlanView | null>(null);
  const [pendingDisable, setPendingDisable] = useState<PlanRequest | null>(null);
  const [sharedSettings, setSharedSettings] = useState<boolean | null>(null);
  const items = ext.workspace?.items ?? [];
  const prepare = async (request: PlanRequest) => {
    if (request.operations.length === 0) return;
    if (request.operations.some((operation) => needsDisableScope(operation, items))) {
      setPendingDisable(request);
      setSharedSettings(null);
      return;
    }
    const result = await ext.preparePlan(request);
    if (result) setView(result);
  };
  const confirmScope = async () => {
    if (!pendingDisable || sharedSettings === null) return;
    const operations = pendingDisable.operations.map((operation) =>
      needsDisableScope(operation, items) ? { ...operation, sharedSettings } : operation,
    );
    const result = await ext.preparePlan({ operations });
    if (result) {
      setPendingDisable(null);
      setView(result);
    }
  };
  const confirm = async () => {
    if (!view) return;
    const outcome = await ext.applyPlan(view.planId);
    if (outcome === null) return;
    setView(null);
    if (
      view.operations.some((operation) => operation.operation === "repair") &&
      outcome.rejected === null &&
      !outcome.rolledBack &&
      outcome.record !== null
    ) {
      await discovery.rescanAfterWrite();
    }
  };
  const restore = async (operationId: string) => {
    const result = await ext.prepareRestore(operationId);
    if (result) setView(result);
  };
  const repair = async (diagnosticIds: string[]) => {
    const result = await discovery.prepareRepair(diagnosticIds);
    if (result) setView(result);
  };
  return {
    view,
    setView,
    prepare,
    confirm,
    restore,
    repair,
    pendingDisable,
    sharedSettings,
    setSharedSettings,
    confirmScope,
    cancelScope: () => setPendingDisable(null),
    close: () => setView(null),
  };
}
