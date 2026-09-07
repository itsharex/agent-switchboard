import { useEffect, useMemo, useRef, useState } from "react";
import type { ObservedExtension } from "../../api/client";
import type { DiscoverScan } from "../../app/extensions/useDiscoverScan";
import { Button } from "../Button";
import { Table, type TableColumn } from "../Table";
import { clientName } from "../../lib/client-name";
import { DiscoveryWarnings } from "./DiscoveryWarnings";
import {
  diagnosticInView,
  observationInView,
  type BindingViewInfo,
  type DiscoveryClientFilter,
  type DiscoveryViewFilter,
} from "./discovery-view";
import { TRANSPORT_LABELS } from "./labels";

interface Props {
  discovery: DiscoverScan;
  busy: boolean;
  /** The active workspace tab and filter bar: the discovery results follow
   * the same view scope as the library list. */
  kindTab: "skill" | "mcp";
  clientFilter: DiscoveryClientFilter;
  search: string;
  /** projectId → display name, for project-scoped rows. */
  projectNames: ReadonlyMap<string, string>;
  /** bindingId → library facts, for managed-binding diagnostics. */
  bindingInfo: ReadonlyMap<string, BindingViewInfo>;
  /** Opens the library detail of the definition managing this row. */
  onViewDetails: (observed: ObservedExtension) => void;
  onImportSkill: (observed: ObservedExtension) => void;
  onImportMcp: (observed: ObservedExtension) => void;
  /** Opens the takeover preview for an unmanaged native item. */
  onTakeover: (observed: ObservedExtension) => void;
  /** Prepares the one-click repair batch for the current view's
   * auto-repairable diagnostics; the page previews it. */
  onRepair: (diagnosticIds: string[]) => void;
}

function originLabel(
  observed: ObservedExtension,
  projectNames: ReadonlyMap<string, string>,
): string {
  switch (observed.origin.origin) {
    case "userRoot":
      return `${clientName(observed.client)} 用户级目录`;
    case "legacyRoot":
      return "历史目录（只读）";
    case "projectRoot": {
      return `项目 ${projectNames.get(observed.origin.projectId) ?? "已登记项目"}`;
    }
    case "managed":
      return "托管安装（只读）";
  }
}

function formatScanTime(scannedAt: string): string {
  const parsed = new Date(scannedAt);
  return Number.isNaN(parsed.getTime())
    ? scannedAt
    : parsed.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** Read-only table of Skills and MCP servers already present on this machine,
 * scoped to the active tab and filter bar: switching tabs reuses the same
 * scan. Actions are the backend-judged ones — 复制到扩展库 saves an
 * independent copy, 管理现有安装 manages the location as-is — and warnings
 * live in one view-scoped collapsible surface. */
export function DiscoverPanel({
  discovery,
  busy,
  kindTab,
  clientFilter,
  search,
  projectNames,
  bindingInfo,
  onViewDetails,
  onImportSkill,
  onImportMcp,
  onTakeover,
  onRepair,
}: Props) {
  const { snapshot, scanning, stale, repairPreparing, ensureInitialScan, scan } = discovery;
  const startedRef = useRef(false);
  const [open, setOpen] = useState(false);
  const [focusDiagnosticId, setFocusDiagnosticId] = useState<string | null>(null);

  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    ensureInitialScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filter: DiscoveryViewFilter = useMemo(
    () => ({ kind: kindTab, client: clientFilter, search }),
    [kindTab, clientFilter, search],
  );

  const observations = useMemo(
    () => (snapshot?.observations ?? []).filter((observed) => observationInView(observed, filter)),
    [snapshot, filter],
  );
  const viewObservationIds = useMemo(
    () => new Set(observations.map((observed) => observed.observationId)),
    [observations],
  );
  const diagnostics = useMemo(
    () =>
      (snapshot?.diagnostics ?? []).filter((diagnostic) =>
        diagnosticInView(diagnostic, filter, viewObservationIds, bindingInfo),
      ),
    [snapshot, filter, viewObservationIds, bindingInfo],
  );
  const diagnosticsByObservation = useMemo(() => {
    const map = new Map<string, { warnings: number; information: number }>();
    for (const diagnostic of diagnostics) {
      if (diagnostic.subject.kind !== "discoveryEntry") continue;
      const current = map.get(diagnostic.subject.observationId) ?? { warnings: 0, information: 0 };
      if (diagnostic.remediation.kind === "info") current.information += 1;
      else current.warnings += 1;
      map.set(diagnostic.subject.observationId, current);
    }
    return map;
  }, [diagnostics]);

  const surfaceDiagnostic = (observationId: string) => {
    const first = diagnostics.find(
      (diagnostic) =>
        diagnostic.subject.kind === "discoveryEntry" &&
        diagnostic.subject.observationId === observationId,
    );
    if (!first) return;
    setFocusDiagnosticId(first.id);
    setOpen(true);
  };

  const offersTakeover = observations.some(
    (observed) =>
      !observed.managed &&
      observed.origin.origin !== "legacyRoot" &&
      observed.origin.origin !== "managed" &&
      observed.actions.takeover.supported,
  );

  const columns: Array<TableColumn<ObservedExtension>> = [
    {
      key: "name",
      header: "名称",
      render: (observed) => (
        <>
          <div>{observed.name}</div>
          {observed.kind === "mcp" && observed.transport && (
            <div className="asb-scope-note">
              {TRANSPORT_LABELS[observed.transport] ?? observed.transport}
            </div>
          )}
          {observed.kind === "skill" && observed.contentDigest && (
            <div className="asb-code">{observed.contentDigest.slice(0, 12)}</div>
          )}
        </>
      ),
    },
    { key: "client", header: "客户端", render: (observed) => clientName(observed.client) },
    {
      key: "scope",
      header: "作用域／项目",
      render: (observed) => originLabel(observed, projectNames),
    },
    {
      key: "managed",
      header: "管理状态",
      render: (observed) => {
        if (observed.managed) return <span className="asb-pill-status">已托管</span>;
        if (observed.origin.origin === "legacyRoot" || observed.origin.origin === "managed") {
          return <span className="asb-pill-status">只读来源</span>;
        }
        return "未托管";
      },
    },
    {
      key: "status",
      header: "警告",
      render: (observed) => {
        const count = diagnosticsByObservation.get(observed.observationId);
        if (count && count.warnings > 0) {
          return (
            <button
              type="button"
              className="asb-warn-text asb-discovery-count"
              aria-label={`查看 ${observed.name} 的 ${count.warnings} 条警告`}
              onClick={() => surfaceDiagnostic(observed.observationId)}
            >
              {count.warnings} 条警告
            </button>
          );
        }
        if (count && count.information > 0) return <span className="asb-scope-note">{count.information} 条提示</span>;
        return "—";
      },
    },
    {
      key: "actions",
      header: "操作",
      render: (observed) => {
        if (observed.managed) {
          return (
            <Button
              variant="secondary"
              disabled={busy}
              aria-label={`查看 ${observed.name} 的管理详情`}
              onClick={() => onViewDetails(observed)}
            >
              查看管理详情
            </Button>
          );
        }
        const readOnly =
          observed.origin.origin === "legacyRoot" || observed.origin.origin === "managed";
        const importSupport = observed.actions.import;
        const takeoverSupport = observed.actions.takeover;
        const importDisabled = !importSupport.supported;
        const reasons = [
          importDisabled ? importSupport.reason : null,
          !readOnly && !takeoverSupport.supported ? takeoverSupport.reason : null,
        ].filter((reason): reason is string => typeof reason === "string");
        return (
          <div>
            <div className="asb-ext-actions">
              {importSupport.inLibrary ? (
                <Button variant="secondary" disabled aria-label={`${observed.name} 已在扩展库`}>
                  已在扩展库
                </Button>
              ) : (
                <Button
                  variant="secondary"
                  disabled={busy || importDisabled}
                  title={importDisabled ? importSupport.reason ?? undefined : undefined}
                  onClick={() =>
                    observed.kind === "mcp" ? onImportMcp(observed) : onImportSkill(observed)
                  }
                >
                  复制到扩展库
                </Button>
              )}
              {!readOnly &&
                (takeoverSupport.supported ? (
                  <Button
                    variant="secondary"
                    disabled={busy}
                    onClick={() => onTakeover(observed)}
                  >
                    管理现有安装
                  </Button>
                ) : (
                  <Button
                    variant="secondary"
                    disabled
                    title={takeoverSupport.reason ?? undefined}
                  >
                    管理现有安装
                  </Button>
                ))}
            </div>
            {reasons.length > 0 && <div className="asb-scope-note">{reasons.join("；")}</div>}
          </div>
        );
      },
    },
  ];

  return (
    <div className="asb-ext-section" aria-label="从本机发现">
      <div className="asb-ext-actions">
        <Button variant="secondary" disabled={busy || scanning} onClick={() => void scan()}>
          重新扫描
        </Button>
        {scanning && <span className="asb-scope-note">正在扫描…</span>}
        {!scanning && snapshot && (
          <span className="asb-scope-note">扫描于 {formatScanTime(snapshot.scannedAt)}</span>
        )}
        {stale && <span className="asb-warn-text">上次扫描失败，结果未更新</span>}
      </div>
      {snapshot && (
        <DiscoveryWarnings
          diagnostics={diagnostics}
          scanId={snapshot.scanId}
          scannedAt={snapshot.scannedAt}
          stale={stale}
          busy={busy}
          repairPreparing={repairPreparing}
          observationNames={new Map(observations.map((row) => [row.observationId, row.name]))}
          bindingInfo={bindingInfo}
          focusDiagnosticId={focusDiagnosticId}
          open={open}
          onOpenChange={setOpen}
          onRepair={() =>
            onRepair(
              diagnostics
                .filter((diagnostic) => diagnostic.remediation.kind === "auto")
                .map((diagnostic) => diagnostic.id),
            )
          }
        />
      )}
      {offersTakeover && (
        <p className="asb-scope-note" role="note">
          提示：管理现有安装已包含加入扩展库，无需先复制。
        </p>
      )}
      {observations.length === 0 ? (
        <p className="asb-empty">
          {snapshot === null
            ? "尚未扫描；点击「重新扫描」查看本机已有的 Skill 或 MCP 服务"
            : `当前${kindTab === "skill" ? " Skills" : " MCP"} 筛选范围下没有本机发现结果`}
        </p>
      ) : (
        <Table
          columns={columns}
          rows={observations}
          rowKey={(observed) => observed.observationId}
          ariaLabel="本机发现的扩展"
        />
      )}
    </div>
  );
}
