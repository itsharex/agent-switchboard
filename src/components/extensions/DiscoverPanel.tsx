import { useEffect, useMemo, useState } from "react";
import type { ObservedExtension } from "../../api/client";
import type { DiscoverScan as ScanState } from "../../app/extensions/useDiscoverScan";
import { discoveryImportMode } from "../../app/extensions/useDiscoveryImport";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Download, RefreshCw } from "lucide-react";
import { DiscoveryWarnings } from "./DiscoveryWarnings";
import { DiscoveryImportList } from "./DiscoveryImportList";
import { useDiscoverySelection } from "./useDiscoverySelection";
import { diagnosticInView, observationInView, type BindingViewInfo } from "./discovery-view";

type DiscoverScan = ScanState & { repairPreparing: boolean };

interface Props {
  discovery: DiscoverScan;
  busy: boolean;
  kindTab: "skill" | "mcp";
  search: string;
  projectNames: ReadonlyMap<string, string>;
  bindingInfo: ReadonlyMap<string, BindingViewInfo>;
  onViewDetails: (observed: ObservedExtension) => void;
  onRepair: (diagnosticIds: string[]) => void;
}

function useDiscoveryRows(props: Props) {
  const { snapshot } = props.discovery;
  return useMemo(() => {
    const filter = { kind: props.kindTab, client: "all" as const, search: props.search };
    const allRows = (snapshot?.observations ?? []).filter((item) => item.kind === props.kindTab);
    const rows = allRows.filter((item) => observationInView(item, filter));
    const ids = new Set(rows.map((item) => item.observationId));
    const diagnostics = (snapshot?.diagnostics ?? [])
      .filter((item) => diagnosticInView(item, filter, ids, props.bindingInfo));
    const warnings = new Map<string, number>();
    for (const item of diagnostics) {
      if (item.subject.kind === "discoveryEntry" && item.remediation.kind !== "info") {
        warnings.set(item.subject.observationId, (warnings.get(item.subject.observationId) ?? 0) + 1);
      }
    }
    return { allRows, rows, diagnostics, warnings };
  }, [snapshot, props.kindTab, props.search, props.bindingInfo]);
}

function ScanToolbar({ discovery, busy }: { discovery: DiscoverScan; busy: boolean }) {
  return <div className="asb-ext-import-scan">
    <span role="status" className="asb-scope-note">{discovery.scanning ? "正在扫描…" : discovery.stale
      ? "上次扫描失败，结果未更新" : discovery.snapshot ? "扫描于 " + new Date(discovery.snapshot.scannedAt)
        .toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "读取本机扩展"}</span>
    <Button variant="icon" aria-label="重新扫描" title="重新扫描" disabled={busy || discovery.scanning}
      onClick={() => void discovery.scan()}><RefreshCw /></Button>
  </div>;
}

function ImportFooter({ props, selection, rows }: {
  props: Props;
  selection: ReturnType<typeof useDiscoverySelection>;
  rows: ObservedExtension[];
}) {
  const selectable = rows.filter((item) => discoveryImportMode(item));
  const all = selectable.length > 0 && selectable.every((item) => selection.selected.has(item.observationId));
  const importing = async () => {
    selection.setResult(null);
    const result = await props.discovery.importSelected(selection.selectedItems);
    if (result) selection.setResult(result);
  };
  return <div className="asb-ext-import-footer">
    <Checkbox label="全选" checked={all} disabled={props.busy || selectable.length === 0}
      onChange={(checked) => selection.selectAll(rows, checked)} />
    <span className="asb-scope-note">已选 {selection.selectedItems.length} 项</span>
    <Button variant="primary" disabled={props.busy || selection.selectedItems.length === 0}
      onClick={() => void importing()}><Download />导入所选（{selection.selectedItems.length}）</Button>
  </div>;
}

/** Native imports retain each detected client installation and its recovery baseline. */
export function DiscoverPanel(props: Props) {
  const { discovery } = props;
  const { snapshot, ensureInitialScan } = discovery;
  const view = useDiscoveryRows(props);
  const selection = useDiscoverySelection(view.allRows, snapshot?.scanId, props.kindTab);
  const [open, setOpen] = useState(false);
  const [focusDiagnosticId, setFocusDiagnosticId] = useState<string | null>(null);
  useEffect(() => { ensureInitialScan(); }, [ensureInitialScan]);
  useEffect(() => { setOpen(false); setFocusDiagnosticId(null); }, [snapshot?.scanId]);
  const surfaceWarning = (id: string) => {
    const first = view.diagnostics.find((item) => item.subject.kind === "discoveryEntry" && item.subject.observationId === id);
    if (first) { setFocusDiagnosticId(first.id); setOpen(true); }
  };
  return <div className="asb-ext-import" aria-label="从本机发现">
    <ScanToolbar discovery={discovery} busy={props.busy} />
    {snapshot && <DiscoveryWarnings diagnostics={view.diagnostics} scanId={snapshot.scanId}
      scannedAt={snapshot.scannedAt} stale={discovery.stale} busy={props.busy}
      repairPreparing={discovery.repairPreparing} bindingInfo={props.bindingInfo}
      observationNames={new Map(view.rows.map((item) => [item.observationId, item.name]))}
      focusDiagnosticId={focusDiagnosticId} open={open} onOpenChange={setOpen}
      onRepair={() => props.onRepair(view.diagnostics.filter((item) => item.remediation.kind === "auto").map((item) => item.id))} />}
    {selection.result && selection.result.failed.length > 0 && <ul className="asb-ext-import-errors" role="alert">
      {selection.result.failed.map((item, index) => <li key={index}>{item.name}：{item.message}</li>)}
    </ul>}
    {view.rows.length === 0 ? <p className="asb-empty">
      {discovery.scanning ? "正在读取本机安装…" : props.search ? "没有符合搜索条件的本机扩展" : "没有发现本机扩展"}
    </p> : <DiscoveryImportList rows={view.rows} selected={selection.selected} busy={props.busy}
      projects={props.projectNames} warnings={view.warnings} onSelect={selection.select}
      onWarning={surfaceWarning} onViewDetails={props.onViewDetails} />}
    <ImportFooter props={props} selection={selection} rows={view.rows} />
  </div>;
}
