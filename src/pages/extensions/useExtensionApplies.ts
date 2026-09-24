import { createElement, useEffect, useRef, useState } from "react";
import type {
  ExtensionOperationRecord,
  ExtensionPlanView,
  ExtensionsWorkspace,
  PlanRequest,
  PlanRequestOperation,
} from "../../api/client";
import {
  applyExtensionPlan,
  prepareExtensionPlan,
  prepareExtensionRepair,
  prepareExtensionRestore,
} from "../../api/client";
import { needsDisableScope } from "../../app/extensions/deployment-state";
import type { DiscoverScan } from "../../app/extensions/useDiscoverScan";
import { ToastMessageList } from "../../components/Toaster";
import type { useExtensions } from "../../app/useExtensions";
import { toast, toastMessage } from "../../components/use-toast";
import { useExtensionApplyConfirmation } from "./useExtensionApplyConfirmation";

export type ExtensionApplyResult =
  | { status: "applied"; record: ExtensionOperationRecord | null; workspace: ExtensionsWorkspace }
  | { status: "rejected"; message: string }
  | { status: "rolledBack"; record: ExtensionOperationRecord | null }
  | { status: "unverified"; record: ExtensionOperationRecord | null }
  | { status: "cancelled" | "failed" | "blocked" | "empty" };

export type ExtensionApplyPhase = "preparing" | "scope" | "confirmation" | "applying" | "refreshing";
type ApplyKind = "change" | "restore" | "repair";
type Extensions = ReturnType<typeof useExtensions>;
type Confirmation = ReturnType<typeof useExtensionApplyConfirmation>;
interface ApplyContext {
  signal: AbortSignal;
  phase: (phase: ExtensionApplyPhase) => void;
  track: (plan: ExtensionPlanView) => void;
}

function plannedOperations(plan: ExtensionPlanView): PlanRequestOperation[] {
  return plan.operations.map((operation) => ({
    operation: operation.operation,
    definitionId: operation.definitionId,
    targets: operation.targets.map((target) => target.target),
  }));
}

function useExtensionApplyExecution(ext: Extensions) {
  const active = useRef<AbortController | null>(null);
  const [phase, setPhase] = useState<ExtensionApplyPhase | null>(null);
  const [pendingKind, setPendingKind] = useState<ApplyKind | null>(null);
  const [pendingOperations, setPendingOperations] = useState<PlanRequestOperation[]>([]);
  useEffect(() => () => active.current?.abort(), []);

  const execute = async (
    kind: ApplyKind,
    operations: PlanRequestOperation[],
    action: (context: ApplyContext) => Promise<ExtensionApplyResult>,
  ): Promise<ExtensionApplyResult> => {
    if (active.current || ext.isBusy()) return { status: "blocked" };
    const controller = new AbortController();
    active.current = controller;
    const { signal } = controller;
    setPhase("preparing");
    setPendingKind(kind);
    setPendingOperations(operations);
    let entered = false;
    const context: ApplyContext = {
      signal,
      phase: (next) => { if (!signal.aborted) setPhase(next); },
      track: (plan) => {
        if (signal.aborted) return;
        const definitions = new Set(operations.map((operation) => operation.definitionId ??
          ext.workspace?.items.find((item) => item.bindings.some((binding) =>
            binding.id === operation.bindingId))?.id));
        setPendingOperations([...operations, ...plannedOperations(plan).filter((operation) =>
          !definitions.has(operation.definitionId ?? undefined))]);
      },
    };
    try {
      const result = await ext.runExclusive(async () => {
        entered = true;
        try {
          return await action(context);
        } catch (caught) {
          if (!signal.aborted) await ext.refresh();
          throw caught;
        }
      });
      if (signal.aborted) return { status: "cancelled" };
      return result ?? { status: entered ? "failed" : "blocked" };
    } finally {
      active.current = null;
      if (!signal.aborted) {
        setPhase(null);
        setPendingKind(null);
        setPendingOperations([]);
      }
    }
  };
  return { execute, phase, pendingKind, pendingOperations };
}

function reportRollback(record: ExtensionOperationRecord | null) {
  const failures = record?.rollback?.failed ?? [];
  const messages = record?.resources.flatMap((resource) => resource.targets.flatMap((target) =>
    target.outcome.kind === "failed" || target.outcome.kind === "restoreFailed"
      ? [target.outcome.message] : [])) ?? [];
  toast({
    kind: "error",
    title: failures.length > 0
      ? toastMessage("extensions.apply.failedPartialRollback")
      : toastMessage("extensions.apply.failedRolledBack"),
    description: messages.length + failures.length > 0
      ? createElement(ToastMessageList, { items: [...messages, ...failures] })
      : toastMessage("extensions.apply.failureDetails"),
  });
}

async function applyPrepared(
  plan: ExtensionPlanView,
  repair: boolean,
  ext: Extensions,
  discovery: DiscoverScan,
  context: ApplyContext,
): Promise<ExtensionApplyResult> {
  context.phase("applying");
  const outcome = await applyExtensionPlan(plan.planId, true);
  context.phase("refreshing");
  const workspace = await ext.refresh();
  if (context.signal.aborted) return { status: "cancelled" };
  if (outcome.rejected !== null) {
    toast({ kind: "warning", title: toastMessage("extensions.apply.rejectedTitle"), description: outcome.rejected });
    return { status: "rejected", message: outcome.rejected };
  }
  if (outcome.rolledBack) {
    reportRollback(outcome.record);
    return { status: "rolledBack", record: outcome.record };
  }
  if (workspace === null || (repair && (await discovery.scan()) === null)) {
    toast({
      kind: "warning",
      title: repair ? toastMessage("extensions.apply.repairUnverified") : toastMessage("extensions.apply.appliedRefreshFailed"),
      description: toastMessage("extensions.apply.refreshHint"),
    });
    return { status: "unverified", record: outcome.record };
  }
  if (context.signal.aborted) return { status: "cancelled" };
  toast({ kind: "success", title: repair ? toastMessage("extensions.apply.repaired") : toastMessage("extensions.apply.applied") });
  return { status: "applied", record: outcome.record, workspace };
}

async function executePrepared(
  plan: ExtensionPlanView,
  repair: boolean,
  ext: Extensions,
  discovery: DiscoverScan,
  confirmation: Confirmation,
  context: ApplyContext,
): Promise<ExtensionApplyResult> {
  if (context.signal.aborted) return { status: "cancelled" };
  context.track(plan);
  const sensitive = plan.operations.some((entry) =>
    entry.targets.some(
      (target) => target.writesSensitiveConnectionData || target.adoptsNativeEntry,
    ));
  if (sensitive) {
    context.phase("confirmation");
    if (await confirmation.requestWrite(plan, context.signal) !== true) return { status: "cancelled" };
  }
  if (context.signal.aborted) return { status: "cancelled" };
  return applyPrepared(plan, repair, ext, discovery, context);
}

/** One lock owns prepare, user confirmation, apply and verification. */
export function useExtensionApplies(ext: Extensions, discovery: DiscoverScan) {
  const confirmation = useExtensionApplyConfirmation();
  const execution = useExtensionApplyExecution(ext);
  const items = ext.workspace?.items ?? [];
  const run = async (request: PlanRequest): Promise<ExtensionApplyResult> => {
    if (request.operations.length === 0) return { status: "empty" };
    return execution.execute("change", request.operations, async (context) => {
      let scoped = request;
      if (request.operations.some((operation) => needsDisableScope(operation, items))) {
        context.phase("scope");
        const sharedSettings = await confirmation.requestScope(request, context.signal);
        if (sharedSettings === null) return { status: "cancelled" };
        scoped = { operations: request.operations.map((operation) =>
          needsDisableScope(operation, items) ? { ...operation, sharedSettings } : operation) };
      }
      if (context.signal.aborted) return { status: "cancelled" };
      context.phase("preparing");
      return executePrepared(await prepareExtensionPlan(scoped), false, ext, discovery, confirmation, context);
    });
  };
  const restore = (operationId: string): Promise<ExtensionApplyResult> => {
    const record = ext.workspace?.history.find((entry) => entry.id === operationId);
    const operations = record?.resources.map((resource) => ({
      operation: "restore" as const,
      definitionId: resource.definitionId,
      targets: resource.targets.map((target) => target.target),
    })) ?? [];
    return execution.execute("restore", operations, async (context) =>
      executePrepared(await prepareExtensionRestore(operationId), false, ext, discovery, confirmation, context));
  };
  const repair = async (diagnosticIds: string[]): Promise<ExtensionApplyResult> => {
    const snapshot = discovery.snapshot;
    if (!snapshot || diagnosticIds.length === 0) return { status: "empty" };
    const operations = snapshot.diagnostics.flatMap((diagnostic) =>
      diagnosticIds.includes(diagnostic.id) && diagnostic.subject.kind === "managedBinding"
        ? [{ operation: "repair" as const, bindingId: diagnostic.subject.bindingId }] : []);
    return execution.execute("repair", operations, async (context) =>
      executePrepared(await prepareExtensionRepair(snapshot.scanId, diagnosticIds), true,
        ext, discovery, confirmation, context));
  };
  return {
    run, restore, repair,
    phase: execution.phase,
    pendingKind: execution.pendingKind,
    pendingOperations: execution.pendingOperations,
    confirmationBusy: execution.phase !== "scope" && execution.phase !== "confirmation",
    ...confirmation.dialog,
  };
}

export type ExtensionApplies = ReturnType<typeof useExtensionApplies>;
