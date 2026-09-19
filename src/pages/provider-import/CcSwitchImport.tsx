import type { CcSwitchImportOutcome, CcSwitchScan, CcSwitchScanItem } from "../../api/client";
import { pickDirectory } from "../../api/client";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { Table, type TableColumn } from "../../components/Table";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";
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

function providerDetail(item: CcSwitchScanItem): string {
  return [clientName(item.app), item.routeMode === "official" ? "官方登录" : null, item.model, item.baseUrl,
    item.usageScriptUpdatesExisting ? "将补充用量查询脚本" : item.usageScriptImportable ? "将导入用量查询脚本" : null,
    item.endpointCandidates > 0 ? `测速候选 ${item.endpointCandidates}` : null,
  ].filter(Boolean).join(" · ");
}

function importRows(scan: CcSwitchScan | null): CcImportRow[] {
  if (!scan) return [];
  return [
    ...scan.providers.map((item) => ({
      key: item.key, item, name: item.name, detail: providerDetail(item),
      status: item.existing
        ? item.app === "codex" && item.routeMode === "custom"
          ? "已存在相同路由，导入将跳过"
          : "已存在相同档案，导入将跳过"
        : null,
      warnings: item.warnings,
    })),
    ...scan.skipped.map((skip) => ({ key: skip.key, item: null, name: skip.name, detail: null,
      status: `无法导入：${skip.reason}`, warnings: [],
    })),
  ];
}

function importColumns({ selected, busy, onSelect }: Omit<CcSwitchImportProps, "scan" | "result" | "onScan" | "onImport">): Array<TableColumn<CcImportRow>> {
  return [
    { key: "provider", header: "供应商", render: (row) => {
      const item = row.item;
      if (!item) return row.name;
      return <Checkbox label={row.name} checked={Boolean(selected[item.key]) && !item.existing}
        disabled={busy || item.existing} onChange={(checked) => onSelect(item.key, checked)} />;
    } },
    { key: "detail", header: "详情", render: (row) => row.detail },
    { key: "status", header: "状态", render: (row) => <>{row.status}
      {row.warnings.map((warning) => <div key={warning} className="asb-warn-text">{warning}</div>)}
    </> },
  ];
}

function ImportResult({ result }: { result: CcSwitchImportOutcome | null }) {
  if (!result) return null;
  return (
    <>
      <div className={`asb-banner ${result.notImported.length > 0 ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status" aria-label="导入结果">
        <span>已导入 {result.importedCount} 项
          {result.usageScriptImportedCount > 0 && ` · 已导入用量脚本 ${result.usageScriptImportedCount} 项`}
          {result.endpointCandidatesImported > 0 && ` · 已导入测速候选 ${result.endpointCandidatesImported} 项`}
          {result.skippedExisting.length > 0 && ` · 跳过已存在 ${result.skippedExisting.length} 项`}
          {result.notImported.length > 0 && ` · 未导入 ${result.notImported.length} 项`}
        </span>
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
  const rows = importRows(props.scan);
  const selectedCount = rows.filter(({ item }) => item && !item.existing && props.selected[item.key]).length;
  const pickFolder = async () => {
    if (props.busy) return;
    const picked = await pickDirectory();
    if (picked) props.onDirectoryChange(picked);
  };
  return (
    <>
      <ModuleHeader
        title="本机数据库"
        primaryActions={
          <>
            <Button variant="secondary" disabled={props.busy} onClick={() => void pickFolder()}>
              选择数据库文件夹
            </Button>
            <Button variant="secondary" disabled={props.busy} onClick={props.onScan}>
              {props.directory ? "扫描所选文件夹（只读）" : "扫描本机数据库（只读）"}
            </Button>
          </>
        }
      />
      {props.directory && (
        <div className="asb-kv">
          <span className="asb-kv-label">所选文件夹</span>
          <span className="asb-kv-value asb-code">{props.directory}</span>
          <div className="asb-kv-actions">
            <Button variant="secondary" disabled={props.busy} onClick={() => props.onDirectoryChange(null)}>
              恢复默认位置
            </Button>
          </div>
        </div>
      )}
      {props.scan ? <div className="asb-ccscan">
        <div className="asb-kv">
          <span className="asb-kv-label">数据库文件</span>
          <span className="asb-kv-value asb-code">{props.scan.dbPath}</span>
        </div>
        {rows.length === 0 ? (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">导入源中没有供应商。</h3>
          </div>
        ) : <Table columns={importColumns(props)} rows={rows} rowKey={(row) => row.key} ariaLabel="本机数据库扫描结果" />}
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
          <h3 className="asb-section-title">扫描后选择可导入的供应商档案；默认读取本机数据库，也可选择其他包含 cc-switch.db 的文件夹。</h3>
        </div>
      )}
      <ImportResult result={props.result} />
    </>
  );
}
