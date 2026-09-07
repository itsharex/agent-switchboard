import { useEffect, useId, useRef } from "react";
import type { ExtensionDiagnostic } from "../../api/client";
import { Button } from "../Button";
import { ChevronDownIcon, ChevronUpIcon } from "../icons";
import { clientName } from "../../lib/client-name";
import { DIAGNOSTIC_CODE_LABELS, REMEDIATION_LABELS } from "./labels";
import type { BindingViewInfo } from "./discovery-view";

interface Props {
  diagnostics: ExtensionDiagnostic[];
  /** Identity of the scan the diagnostics belong to; collapsing resets on
   * every new scan. */
  scanId: string;
  scannedAt: string | null;
  /** True when the last scan attempt failed and these facts were not
   * refreshed. */
  stale: boolean;
  busy: boolean;
  repairPreparing: boolean;
  /** observationId → display name, for entry-subject diagnostics. */
  observationNames: ReadonlyMap<string, string>;
  /** bindingId → library facts, for managed-binding diagnostics. */
  bindingInfo: ReadonlyMap<string, BindingViewInfo>;
  /** A diagnostic id the table wants surfaced; it is highlighted while set. */
  focusDiagnosticId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRepair: () => void;
}

function subjectLabel(
  diagnostic: ExtensionDiagnostic,
  observationNames: ReadonlyMap<string, string>,
  bindingInfo: ReadonlyMap<string, BindingViewInfo>,
): string {
  switch (diagnostic.subject.kind) {
    case "discoveryEntry":
      return observationNames.get(diagnostic.subject.observationId) ?? "已发现的条目";
    case "managedBinding":
      return bindingInfo.get(diagnostic.subject.bindingId)?.name ?? "已托管的扩展";
    case "scanLocation":
      return diagnostic.subject.label;
  }
}

function remediationText(diagnostic: ExtensionDiagnostic): string {
  switch (diagnostic.remediation.kind) {
    case "auto":
      return `${REMEDIATION_LABELS.auto}：${diagnostic.remediation.reason}`;
    case "manual":
      return `${REMEDIATION_LABELS.manual}：${diagnostic.remediation.reason}`;
    case "info":
      return REMEDIATION_LABELS.info;
  }
}

/** The single collapsible surface for discovery diagnostics. Collapsed it is
 * exactly one line — the counted summary plus the repair and toggle actions;
 * expanded it lists every diagnostic with its own identity, reason, and
 * remediation class. */
export function DiscoveryWarnings({
  diagnostics,
  scanId,
  scannedAt,
  stale,
  busy,
  repairPreparing,
  observationNames,
  bindingInfo,
  focusDiagnosticId,
  open,
  onOpenChange,
  onRepair,
}: Props) {
  const detailsId = useId();
  const summaryTextId = useId();
  const lastScanIdRef = useRef(scanId);

  // Every new scan starts collapsed again: the counted summary is the
  // resting state, never a persisted setting. The very first mount keeps
  // whatever the parent chose.
  useEffect(() => {
    if (lastScanIdRef.current === scanId) return;
    lastScanIdRef.current = scanId;
    onOpenChange(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanId]);

  const warnings = diagnostics.filter(
    (diagnostic) => diagnostic.remediation.kind !== "info",
  );
  const informationCount = diagnostics.length - warnings.length;
  if (warnings.length === 0 && informationCount === 0) return null;

  const repairable = warnings.filter(
    (diagnostic) => diagnostic.remediation.kind === "auto",
  ).length;
  const summary = `当前结果有 ${warnings.length} 条警告，${repairable} 条可修复${
    informationCount > 0 ? `，${informationCount} 条提示` : ""
  }`;

  return (
    <section className="asb-discovery-warnings" aria-label="发现警告">
      <div className="asb-banner asb-banner-warning asb-discovery-summary">
        <button
          type="button"
          className="asb-banner-toggle asb-discovery-summary-toggle"
          aria-expanded={open}
          aria-controls={detailsId}
          aria-describedby={summaryTextId}
          aria-label={open ? "收起警告详情" : `展开警告详情：${summary}`}
          onClick={() => onOpenChange(!open)}
        >
          {open ? <ChevronUpIcon /> : <ChevronDownIcon />}
          <span id={summaryTextId} className="asb-discovery-summary-text">
            {summary}
            {stale && "（上次扫描未更新）"}
          </span>
        </button>
        <div className="asb-discovery-summary-actions">
          {repairable > 0 ? (
            <Button
              variant="primary"
              disabled={busy || repairPreparing}
              aria-label={`修复这 ${repairable} 项可修复警告`}
              onClick={onRepair}
            >
              {repairPreparing ? "正在准备…" : `修复这 ${repairable} 项`}
            </Button>
          ) : warnings.length > 0 ? (
            <span className="asb-scope-note">没有可自动修复项，展开查看各项处理方式</span>
          ) : null}
          <Button
            variant="secondary"
            disabled={busy}
            aria-controls={detailsId}
            aria-expanded={open}
            onClick={() => onOpenChange(!open)}
          >
            {open ? "收起" : "展开"}
          </Button>
        </div>
      </div>
      {stale && scannedAt && (
        <p className="asb-scope-note" role="status">
          上次扫描结果生成于 {scannedAt}；本次扫描失败，结果未更新。
        </p>
      )}
      {open && (
        <ul id={detailsId} className="asb-discovery-diagnostic-list">
          {diagnostics.map((diagnostic) => {
            const focused =
              focusDiagnosticId !== null && focusDiagnosticId === diagnostic.id;
            return (
              <li
                key={diagnostic.id}
                className={
                  focused
                    ? "asb-discovery-diagnostic is-focused"
                    : "asb-discovery-diagnostic"
                }
              >
                <p className="asb-discovery-diagnostic-head">
                  <span className="asb-pill-status">
                    {DIAGNOSTIC_CODE_LABELS[diagnostic.code] ?? diagnostic.code}
                  </span>
                  <strong>{subjectLabel(diagnostic, observationNames, bindingInfo)}</strong>
                  <span className="asb-scope-note">{clientName(diagnostic.client)}</span>
                </p>
                <p>{diagnostic.message}</p>
                <p className="asb-scope-note">{remediationText(diagnostic)}</p>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
