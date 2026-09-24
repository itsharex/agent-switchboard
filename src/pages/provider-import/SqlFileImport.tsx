import type { ProviderSqlImportOutcome, ProviderSqlScan, ProviderSqlScanItem } from "../../api/client";
import { pickFile } from "../../api/client";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { Table, type TableColumn } from "../../components/Table";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";
import { useI18n, type TFunction } from "../../i18n";

function kindLabel(kind: ProviderSqlScanItem["kind"], t: TFunction): string {
  switch (kind) {
    case "claude": return "Claude";
    case "codex_official": return t("importDiscovery.sql.kind.codexOfficial");
    case "codex_custom": return t("importDiscovery.sql.kind.codexCustom");
  }
}

interface SqlImportRow {
  key: string;
  item: ProviderSqlScanItem | null;
  name: string;
  detail: string | null;
  status: string | null;
  warnings: string[];
}

interface SqlFileImportProps {
  scan: ProviderSqlScan | null;
  selected: Record<string, boolean>;
  result: ProviderSqlImportOutcome | null;
  busy: boolean;
  onSelect: (key: string, checked: boolean) => void;
  onApply: (path: string) => void;
  onImport: () => void;
}

function providerDetail(item: ProviderSqlScanItem, t: TFunction): string {
  return [kindLabel(item.kind, t), item.model, item.baseUrl].filter(Boolean).join(" · ");
}

function importRows(scan: ProviderSqlScan | null, t: TFunction): SqlImportRow[] {
  if (!scan) return [];
  return [
    ...scan.providers.map((item) => ({
      key: item.key, item, name: item.name, detail: providerDetail(item, t),
      status: item.existing ? t("importDiscovery.sql.exists") : t("importDiscovery.sql.new"),
      warnings: item.warnings,
    })),
    ...scan.skipped.map((skip) => ({ key: skip.name, item: null, name: skip.name, detail: null,
      status: t("importDiscovery.status.skipped", { reason: skip.reason }), warnings: [],
    })),
  ];
}

function importColumns({ selected, busy, onSelect }: Omit<SqlFileImportProps, "scan" | "result" | "onApply" | "onImport">, t: TFunction): Array<TableColumn<SqlImportRow>> {
  return [
    { key: "provider", header: t("importDiscovery.label.provider"), render: (row) => {
      const item = row.item;
      if (!item) return row.name;
      return <Checkbox label={row.name} checked={Boolean(selected[item.key])}
        disabled={busy} onChange={(checked) => onSelect(item.key, checked)} />;
    } },
    { key: "detail", header: t("importDiscovery.label.detail"), render: (row) => row.detail },
    { key: "status", header: t("importDiscovery.label.status"), render: (row) => <>{row.status}
      {row.warnings.map((warning) => <div key={warning} className="asb-warn-text">{warning}</div>)}
    </> },
  ];
}

function ImportResult({ result }: { result: ProviderSqlImportOutcome | null }) {
  const { t } = useI18n();
  if (!result) return null;
  return (
    <>
      <div className={`asb-banner ${result.notImported.length > 0 ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status" aria-label={t("importDiscovery.result.aria")}>
        <span>{t("importDiscovery.result.imported", { count: result.importedCount })}
          {result.updatedCount > 0 && ` · ${t("importDiscovery.result.updated", { count: result.updatedCount })}`}
          {result.notImported.length > 0 && ` · ${t("importDiscovery.result.notImported", { count: result.notImported.length })}`}
        </span>
      </div>
      {result.notImported.length > 0 && <div className="asb-ccscan">
        {result.notImported.map((skip, index) => <div className="asb-kv" key={`${skip.name}-${index}`}>
          <span className="asb-kv-label">{skip.name}</span>
          <span className="asb-kv-value asb-warn-text">{skip.reason}</span>
        </div>)}
      </div>}
    </>
  );
}

/** Applies one exported SQL file, previews its complete provider records,
 * and imports the selection. Existing records with the same id are explicit
 * opt-in overwrites; the file itself stays backend-read only. */
export function SqlFileImport(props: SqlFileImportProps) {
  const { t } = useI18n();
  const rows = importRows(props.scan, t);
  const selectedCount = rows.filter(({ item }) => item && props.selected[item.key]).length;
  const pickSqlFile = async () => {
    if (props.busy) return;
    const picked = await pickFile(t("importDiscovery.sql.sqlFile"), ["sql"]);
    if (picked) props.onApply(picked);
  };
  return (
    <>
      <ModuleHeader
        title={t("importDiscovery.tab.sqlImport")}
        primaryActions={
          <Button variant="secondary" disabled={props.busy} onClick={() => void pickSqlFile()}>
            {t("importDiscovery.sql.pickFile")}
          </Button>
        }
      />
      {props.scan ? <div className="asb-ccscan">
        <div className="asb-kv">
          <span className="asb-kv-label">{t("importDiscovery.sql.sqlFile")}</span>
          <span className="asb-kv-value asb-code">{props.scan.sqlPath}</span>
        </div>
        {rows.length === 0 ? (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">{t("importDiscovery.sql.empty")}</h3>
          </div>
        ) : <Table columns={importColumns(props, t)} rows={rows} rowKey={(row) => row.key} ariaLabel={t("importDiscovery.sql.tableAria")} />}
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
          <h3 className="asb-section-title">{t("importDiscovery.sql.emptyHint")}</h3>
        </div>
      )}
      <ImportResult result={props.result} />
    </>
  );
}
