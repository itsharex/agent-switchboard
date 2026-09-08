import type { CcSwitchImportOutcome, CcSwitchScan, CcSwitchScanItem } from "../../api/client";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { Table, type TableColumn } from "../../components/Table";
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
  onSelect: (key: string, checked: boolean) => void;
  onScan: () => void;
  onImport: () => void;
}

function providerDetail(item: CcSwitchScanItem): string {
  return [clientName(item.app), item.routeMode === "official" ? "官方登录" : null, item.model, item.baseUrl,
    item.usageScriptUpdatesExisting ? "将补充用量查询脚本" : item.usageScriptImportable ? "将导入用量查询脚本" : null,
  ].filter(Boolean).join(" · ");
}

function importRows(scan: CcSwitchScan | null): CcImportRow[] {
  if (!scan) return [];
  return [
    ...scan.providers.map((item) => ({
      key: item.key, item, name: item.name, detail: providerDetail(item),
      status: item.existing ? "已存在相同档案，导入将跳过" : null, warnings: item.warnings,
    })),
    ...scan.skipped.map((skip) => ({ key: skip.key, item: null, name: skip.name, detail: null,
      status: `无法导入：${skip.reason}`, warnings: [],
    })),
  ];
}

function importColumns({ selected, busy, onSelect }: Pick<CcSwitchImportProps, "selected" | "busy" | "onSelect">): Array<TableColumn<CcImportRow>> {
  return [
    { key: "provider", header: "供应商", render: (row) => {
      const item = row.item;
      return item ? <Checkbox label={row.name} checked={Boolean(selected[item.key]) && !item.existing}
        disabled={busy || item.existing} onChange={(checked) => onSelect(item.key, checked)} /> : row.name;
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

export function CcSwitchImport(props: CcSwitchImportProps) {
  const rows = importRows(props.scan);
  const selectedCount = rows.filter(({ item }) => item && !item.existing && props.selected[item.key]).length;
  return (
    <section className="asb-panel" aria-label="从 CC Switch 导入">
      <div className="asb-panel-heading">
        <h2 className="asb-panel-title">CC Switch</h2>
        <Button variant="secondary" disabled={props.busy} onClick={props.onScan}>扫描 CC Switch（只读）</Button>
      </div>
      {props.scan ? <div className="asb-ccscan">
        {rows.length === 0 ? <p className="asb-empty">CC Switch 中没有供应商。</p>
          : <Table columns={importColumns(props)} rows={rows} rowKey={(row) => row.key} ariaLabel="CC Switch 扫描结果" />}
        <div className="asb-form-actions">
          <Button variant="primary" disabled={props.busy || selectedCount === 0} onClick={props.onImport}>
            导入所选 {selectedCount} 项
          </Button>
        </div>
      </div> : <p className="asb-empty">扫描后选择要导入的 Codex 与 Claude 档案。</p>}
      <ImportResult result={props.result} />
    </section>
  );
}
