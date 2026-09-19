import type { ProviderSqlImportOutcome, ProviderSqlScan, ProviderSqlScanItem } from "../../api/client";
import { pickFile } from "../../api/client";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { Table, type TableColumn } from "../../components/Table";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";

const KIND_LABELS: Record<ProviderSqlScanItem["kind"], string> = {
  claude: "Claude",
  codex_official: "Codex 官方登录",
  codex_custom: "Codex 第三方",
};

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

function providerDetail(item: ProviderSqlScanItem): string {
  return [KIND_LABELS[item.kind], item.model, item.baseUrl].filter(Boolean).join(" · ");
}

function importRows(scan: ProviderSqlScan | null): SqlImportRow[] {
  if (!scan) return [];
  return [
    ...scan.providers.map((item) => ({
      key: item.key, item, name: item.name, detail: providerDetail(item),
      status: item.existing ? "已存在，导入将覆盖更新" : "新增",
      warnings: item.warnings,
    })),
    ...scan.skipped.map((skip) => ({ key: skip.name, item: null, name: skip.name, detail: null,
      status: `无法导入：${skip.reason}`, warnings: [],
    })),
  ];
}

function importColumns({ selected, busy, onSelect }: Omit<SqlFileImportProps, "scan" | "result" | "onApply" | "onImport">): Array<TableColumn<SqlImportRow>> {
  return [
    { key: "provider", header: "供应商", render: (row) => {
      const item = row.item;
      if (!item) return row.name;
      return <Checkbox label={row.name} checked={Boolean(selected[item.key])}
        disabled={busy} onChange={(checked) => onSelect(item.key, checked)} />;
    } },
    { key: "detail", header: "详情", render: (row) => row.detail },
    { key: "status", header: "状态", render: (row) => <>{row.status}
      {row.warnings.map((warning) => <div key={warning} className="asb-warn-text">{warning}</div>)}
    </> },
  ];
}

function ImportResult({ result }: { result: ProviderSqlImportOutcome | null }) {
  if (!result) return null;
  return (
    <>
      <div className={`asb-banner ${result.notImported.length > 0 ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status" aria-label="导入结果">
        <span>已导入 {result.importedCount} 项
          {result.updatedCount > 0 && ` · 覆盖更新 ${result.updatedCount} 项`}
          {result.notImported.length > 0 && ` · 未导入 ${result.notImported.length} 项`}
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
  const rows = importRows(props.scan);
  const selectedCount = rows.filter(({ item }) => item && props.selected[item.key]).length;
  const pickSqlFile = async () => {
    if (props.busy) return;
    const picked = await pickFile("SQL 文件", ["sql"]);
    if (picked) props.onApply(picked);
  };
  return (
    <>
      <ModuleHeader
        title="导入 SQL"
        primaryActions={
          <Button variant="secondary" disabled={props.busy} onClick={() => void pickSqlFile()}>
            选择导出的 SQL 文件
          </Button>
        }
      />
      {props.scan ? <div className="asb-ccscan">
        <div className="asb-kv">
          <span className="asb-kv-label">SQL 文件</span>
          <span className="asb-kv-value asb-code">{props.scan.sqlPath}</span>
        </div>
        {rows.length === 0 ? (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">导出文件中没有供应商。</h3>
          </div>
        ) : <Table columns={importColumns(props)} rows={rows} rowKey={(row) => row.key} ariaLabel="导出文件预览" />}
        <div className="asb-form-actions">
          <Button variant="primary" disabled={props.busy || selectedCount === 0} onClick={props.onImport}>
            导入所选 {selectedCount} 项
          </Button>
        </div>
      </div> : (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <SearchIcon />
          </span>
          <h3 className="asb-section-title">选择在其他设备导出的 SQL 文件，即可预览并导入全部供应商的完整配置。</h3>
        </div>
      )}
      <ImportResult result={props.result} />
    </>
  );
}
