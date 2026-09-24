import type { CcSwitchImportOutcome, CcSwitchScan, CcSwitchScanItem } from "../../api/client";
import { pickDirectory } from "../../api/client";
import { CcOutcomeText, ccOutcomeNeedsAttention } from "../../app/cc-import-outcome";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { Table, type TableColumn } from "../../components/Table";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";
import { useI18n, type TFunction } from "../../i18n";
import { clientName } from "../../lib/client-name";

interface CcImportRow {
  key: string;
  item: CcSwitchScanItem | null;
  name: string;
  detail: string | null;
  status: string | null;
  warnings: string[];
}

interface CcSwitchImportProps {
  scan: CcSwitchScan | null;
  selected: Record<string, boolean>;
  result: CcSwitchImportOutcome | null;
  busy: boolean;
  /** Picked source folder containing `cc-switch.db`; null keeps the default. */
  directory: string | null;
  onSelect: (key: string, checked: boolean) => void;
  onDirectoryChange: (directory: string | null) => void;
  onScan: () => void;
  onImport: () => void;
}

function providerDetail(item: CcSwitchScanItem, t: TFunction): string {
  return [clientName(item.app), item.routeMode === "official" ? t("importDiscovery.label.official") : null, item.model, item.baseUrl,
    item.usageScriptUpdatesExisting ? t("importDiscovery.cc.usagePatch") : item.usageScriptImportable ? t("importDiscovery.cc.usageImport") : null,
    item.endpointCandidates > 0 ? t("importDiscovery.cc.endpointCandidates", { count: item.endpointCandidates }) : null,
  ].filter(Boolean).join(" · ");
}

function importRows(scan: CcSwitchScan | null, t: TFunction): CcImportRow[] {
  if (!scan) return [];
  return [
    ...scan.providers.map((item) => ({
      key: item.key, item, name: item.name, detail: providerDetail(item, t),
      status: item.existing
        ? item.app === "codex" && item.routeMode === "custom"
          ? t("importDiscovery.cc.existsRoute")
          : t("importDiscovery.cc.existsProfile")
        : null,
      warnings: item.warnings,
    })),
    ...scan.skipped.map((skip) => ({ key: skip.key, item: null, name: skip.name, detail: null,
      status: t("importDiscovery.status.skipped", { reason: skip.reason }), warnings: [],
    })),
  ];
}

function importColumns({ selected, busy, onSelect }: Omit<CcSwitchImportProps, "scan" | "result" | "onScan" | "onImport">, t: TFunction): Array<TableColumn<CcImportRow>> {
  return [
    { key: "provider", header: t("importDiscovery.label.provider"), render: (row) => {
      const item = row.item;
      if (!item) return row.name;
      return <Checkbox label={row.name} checked={Boolean(selected[item.key]) && !item.existing}
        disabled={busy || item.existing} onChange={(checked) => onSelect(item.key, checked)} />;
    } },
    { key: "detail", header: t("importDiscovery.label.detail"), render: (row) => row.detail },
    { key: "status", header: t("importDiscovery.label.status"), render: (row) => <>{row.status}
      {row.warnings.map((warning) => <div key={warning} className="asb-warn-text">{warning}</div>)}
    </> },
  ];
}

function ImportResult({ result }: { result: CcSwitchImportOutcome | null }) {
  const { t } = useI18n();
  if (!result) return null;
  return (
    <>
      <div className={`asb-banner ${ccOutcomeNeedsAttention(result) ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status" aria-label={t("importDiscovery.result.aria")}>
        <span><CcOutcomeText result={result} /></span>
      </div>
      {result.notImported.length > 0 && <div className="asb-ccscan">
        {result.notImported.map((skip) => <div className="asb-kv" key={skip.key}>
          <span className="asb-kv-label">{skip.name}</span>
          <span className="asb-kv-value asb-warn-text">{skip.reason}</span>
        </div>)}
      </div>}
    </>
  );
}

/** One click imports every selected row: Claude relays, third-party Codex
 * rows (completed inside the backend), and the Codex official record. The
 * source folder defaults to the home database location and stays editable
 * through the native directory picker. */
export function CcSwitchImport(props: CcSwitchImportProps) {
  const { t } = useI18n();
  const rows = importRows(props.scan, t);
  const selectedCount = rows.filter(({ item }) => item && !item.existing && props.selected[item.key]).length;
  const pickFolder = async () => {
    if (props.busy) return;
    const picked = await pickDirectory();
    if (picked) props.onDirectoryChange(picked);
  };
  return (
    <>
      <ModuleHeader
        title={t("importDiscovery.tab.database")}
        primaryActions={
          <>
            <Button variant="secondary" disabled={props.busy} onClick={() => void pickFolder()}>
              {t("importDiscovery.cc.pickFolder")}
            </Button>
            <Button variant="secondary" disabled={props.busy} onClick={props.onScan}>
              {props.directory ? t("importDiscovery.cc.scanPicked") : t("importDiscovery.cc.scanDefault")}
            </Button>
          </>
        }
      />
      {props.directory && (
        <div className="asb-kv">
          <span className="asb-kv-label">{t("importDiscovery.cc.chosenFolder")}</span>
          <span className="asb-kv-value asb-code">{props.directory}</span>
          <div className="asb-kv-actions">
            <Button variant="secondary" disabled={props.busy} onClick={() => props.onDirectoryChange(null)}>
              {t("importDiscovery.cc.resetFolder")}
            </Button>
          </div>
        </div>
      )}
      {props.scan ? <div className="asb-ccscan">
        <div className="asb-kv">
          <span className="asb-kv-label">{t("importDiscovery.cc.dbFile")}</span>
          <span className="asb-kv-value asb-code">{props.scan.dbPath}</span>
        </div>
        {rows.length === 0 ? (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">{t("importDiscovery.cc.empty")}</h3>
          </div>
        ) : <Table columns={importColumns(props, t)} rows={rows} rowKey={(row) => row.key} ariaLabel={t("importDiscovery.cc.tableAria")} />}
        <div className="asb-form-actions">
          <Button variant="primary" disabled={props.busy || selectedCount === 0} onClick={props.onImport}>
            {t("importDiscovery.action.importSelected", { count: selectedCount })}
          </Button>
        </div>
      </div> : (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <SearchIcon />
          </span>
          <h3 className="asb-section-title">{t("importDiscovery.cc.emptyHint")}</h3>
        </div>
      )}
      <ImportResult result={props.result} />
    </>
  );
}
