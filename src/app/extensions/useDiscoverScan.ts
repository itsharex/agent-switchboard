import { useCallback, useRef, useState } from "react";
import {
  discoverExtensions,
  importDiscoveredMcp,
  importDiscoveredSkill,
  prepareExtensionRepair,
  previewDiscoveredTakeover,
  takeoverDiscoveredExtension,
  type CommandError,
  type ExtensionDiscovery,
  type ExtensionKind,
  type ExtensionMutation,
  type ExtensionPlanView,
  type ObservedExtension,
  type TakeoverPreview,
} from "../../api/client";
import { toast } from "../../components/use-toast";
import type { ExclusiveRunner, WorkspaceRefresher } from "./extension-ops";

interface DiscoverScanDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
  onError: (error: CommandError) => void;
}

export interface DiscoveryTakeover {
  target: ObservedExtension;
  preview: TakeoverPreview;
}

/** Owns the discovery scan snapshot, its repair preparation, and the
 * takeover confirmation state that the discover panel drives. A successful
 * scan replaces the snapshot wholesale; a failed scan keeps the previous
 * snapshot and marks it stale. */
export function useDiscoverScan({ refresh, runExclusive, onError }: DiscoverScanDeps) {
  const [snapshot, setSnapshot] = useState<ExtensionDiscovery | null>(null);
  const [scanning, setScanning] = useState(false);
  const [stale, setStale] = useState(false);
  const [repairPreparing, setRepairPreparing] = useState(false);
  const [takeover, setTakeover] = useState<DiscoveryTakeover | null>(null);
  const scanStartedRef = useRef(false);
  const scanInFlightRef = useRef(false);

  const scan = useCallback(async (): Promise<ExtensionDiscovery | null> => {
    // The synchronous in-flight mark blocks repeated clicks while one scan
    // is still running.
    if (scanInFlightRef.current) return null;
    scanInFlightRef.current = true;
    setScanning(true);
    try {
      const next = await discoverExtensions();
      setSnapshot(next);
      setStale(false);
      return next;
    } catch (caught) {
      // Keep the previous snapshot so the user still sees the last facts;
      // the stale mark says they were not refreshed.
      setStale(true);
      onError(caught as CommandError);
      return null;
    } finally {
      scanInFlightRef.current = false;
      setScanning(false);
    }
  }, [onError]);

  /** The panel scans once when it first becomes visible. */
  const ensureInitialScan = useCallback(() => {
    if (scanStartedRef.current || snapshot !== null) return;
    scanStartedRef.current = true;
    void scan();
  }, [scan, snapshot]);

  /** After a confirmed write: refresh the library, then re-scan so the new
   * diagnostics decide which problems actually disappeared. A failed
   * refresh or scan is reported as "executed, unverified" instead of
   * success. */
  const rescanAfterWrite = useCallback(async () => {
    const refreshed = await refresh();
    if (refreshed === null) {
      toast({
        kind: "warning",
        title: "修复已执行，验证未完成",
        description: "扩展状态刷新失败；请稍后重新扫描确认修复结果。",
      });
      return;
    }
    if ((await scan()) === null) {
      toast({
        kind: "warning",
        title: "修复已执行，验证未完成",
        description: "重新扫描失败；请稍后手动重新扫描确认修复结果。",
      });
    }
  }, [refresh, scan]);

  /** Prepares the repair batch for the caller's diagnostic selection — the
   * current view's auto-repairable items, never other tabs' objects. */
  const prepareRepair = useCallback(
    async (diagnosticIds: string[]): Promise<ExtensionPlanView | null> => {
      if (!snapshot || diagnosticIds.length === 0) return null;
      return runExclusive(() => {
        setRepairPreparing(true);
        return prepareExtensionRepair(snapshot.scanId, diagnosticIds).finally(() => {
          setRepairPreparing(false);
        });
      });
    },
    [runExclusive, snapshot],
  );

  const importSkill = useCallback(
    (observed: ObservedExtension) =>
      runExclusive(async (): Promise<boolean> => {
        if (!observed.contentDigest) {
          toast({ kind: "warning", title: "该条目没有可复制的内容摘要" });
          return false;
        }
        const definition = await importDiscoveredSkill(observed.observationId);
        await refresh();
        // Re-scan so the row's backend-judged state (已在扩展库) reflects
        // the copy immediately.
        await scan();
        toast({ kind: "success", title: "已复制到扩展库", description: definition.name });
        return true;
      }),
    [refresh, runExclusive, scan],
  );

  const importMcp = useCallback(
    (observed: ObservedExtension) =>
      runExclusive(async (): Promise<boolean> => {
        const definition = await importDiscoveredMcp(observed.observationId);
        await refresh();
        await scan();
        toast({
          kind: "success",
          title: "已复制到扩展库",
          description: "本机现有安装保持独立；管理现有安装不会改写它。",
        });
        return definition !== null;
      }),
    [refresh, runExclusive, scan],
  );

  const requestTakeover = useCallback(
    (observed: ObservedExtension) =>
      runExclusive(async (): Promise<boolean> => {
        const preview = await previewDiscoveredTakeover(observed.observationId);
        if (preview) setTakeover({ target: observed, preview });
        return preview !== null;
      }),
    [runExclusive],
  );

  const confirmTakeover = useCallback(
    async (): Promise<{ definition: ExtensionMutation; kind: ExtensionKind } | null> => {
    if (!takeover) return null;
    const kind: ExtensionKind = takeover.preview.kind;
    const target = takeover.target;
    const definition = await runExclusive(async () =>
      takeoverDiscoveredExtension(target.observationId),
    );
    if (!definition) return null;
    setTakeover(null);
    const refreshed = await refresh();
    if (refreshed === null || (await scan()) === null) {
      toast({
        kind: "warning",
        title: "管理已完成，发现结果未更新",
        description: "请稍后重新扫描确认管理状态。",
      });
    }
    toast({
      kind: "success",
      title: "已接管本机扩展",
      description: "客户端文件保持原样；移除绑定时将恢复接管时的原始内容。",
    });
      return { definition, kind };
    },
    [refresh, runExclusive, scan, takeover],
  );

  const cancelTakeover = useCallback(() => setTakeover(null), []);

  return {
    snapshot,
    scanning,
    stale,
    repairPreparing,
    takeover,
    ensureInitialScan,
    scan,
    rescanAfterWrite,
    prepareRepair,
    importSkill,
    importMcp,
    requestTakeover,
    confirmTakeover,
    cancelTakeover,
  };
}

export type DiscoverScan = ReturnType<typeof useDiscoverScan>;
