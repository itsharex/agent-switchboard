import { Fragment, useEffect, useState } from "react";
import { backupDiff, type BackupRecord, type KeyChange } from "../api/client";
import { DiffView } from "./DiffView";
import { Button } from "./Button";
import { ConfirmSheet } from "./ConfirmSheet";
import { type TableColumn } from "./Table";
import { Time } from "./Time";
import { RestoreIcon } from "./icons";
import { cx } from "@/utils/cx";

interface Props {
  records: BackupRecord[];
  busy: boolean;
  onRestore: (backupId: string) => void;
}

function reasonLabel(reason: string): string {
  if (reason === "switch") return "切换前备份";
  if (reason === "provider-projection") return "供应商切换前备份";
  if (reason === "restore-precheck") return "恢复前备份";
  if (reason === "gateway-port-change") return "网关端口修改前备份";
  if (reason === "gateway-port-rollback") return "网关端口恢复前备份";
  if (reason === "client-configuration-apply") return "应用客户端配置前备份";
  if (reason === "client-configuration-repair") return "自动修复客户端配置前备份";
  if (reason === "client-configuration-native-defaults") return "恢复客户端原生默认值前备份";
  if (reason === "client-configuration-native-defaults-unmanaged") return "恢复默认值并移除界面外字段前备份";
  if (reason === "client-configuration-clear-extra-configuration") return "清空额外通用配置前备份";
  return reason;
}

function isGatewayPortBackup(reason: string): boolean {
  return reason === "gateway-port-change" || reason === "gateway-port-rollback";
}

function clientLabel(app: string): string {
  return app === "codex" ? "Codex" : "Claude";
}

/** One backup's owned-key difference, fetched while its region stays open. */
function BackupDiff({ record }: { record: BackupRecord }) {
  const [changes, setChanges] = useState<KeyChange[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    backupDiff(record.id)
      .then((next) => {
        if (active) setChanges(next);
      })
      .catch((caught) => {
        if (active) setError((caught as { message?: string }).message ?? "无法生成差异");
      });
    return () => {
      active = false;
    };
  }, [record]);

  if (error) return <p className="asb-warn-text">{error}</p>;
  /* Stands in for the DiffView rows, so it keeps a multi-row footprint. */
  if (changes === null) {
    return <div className="asb-skeleton asb-backup-diff-loading" aria-hidden="true" />;
  }
  if (changes.length === 0) return <p className="asb-empty">与当前文件一致</p>;
  return <DiffView changes={changes} label="当前文件与备份的差异" />;
}

/** Recent validation and restore history (DESIGN.md §7 bottom band). The
 * table markup mirrors the shared Table contract because an open diff needs
 * a full-width expansion row under its own row, which the shared renderer
 * does not emit. */
export function BackupHistory({ records, busy, onRestore }: Props) {
  const [pending, setPending] = useState<BackupRecord | null>(null);
  const [openDiffs, setOpenDiffs] = useState<ReadonlySet<string>>(() => new Set());

  const toggleDiff = (recordId: string) => {
    setOpenDiffs((current) => {
      const next = new Set(current);
      if (next.has(recordId)) {
        next.delete(recordId);
      } else {
        next.add(recordId);
      }
      return next;
    });
  };

  const columns: Array<TableColumn<BackupRecord>> = [
    {
      key: "createdAt",
      header: "时间",
      cellClassName: "asb-code",
      render: (record) => <Time iso={record.createdAt} />,
    },
    { key: "app", header: "客户端", render: (record) => clientLabel(record.app) },
    { key: "reason", header: "原因", render: (record) => reasonLabel(record.reason) },
    {
      key: "contentHash",
      header: "内容哈希",
      cellClassName: "asb-code",
      render: (record) => record.contentHash.slice(0, 12),
    },
    {
      key: "actions",
      header: "操作",
      render: (record) => {
        const isGatewayPortChange = isGatewayPortBackup(record.reason);
        return (
          <div className="asb-backup-actions">
            {isGatewayPortChange ? (
              <span className="asb-scope-note">请在设置的本机网关中修改端口</span>
            ) : (
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => setPending(record)}
              >
                恢复
              </Button>
            )}
            <Button
              variant="secondary"
              disabled={busy}
              aria-expanded={openDiffs.has(record.id)}
              onClick={() => toggleDiff(record.id)}
            >
              查看差异
            </Button>
          </div>
        );
      },
    },
  ];

  if (records.length === 0) {
    return (
      <div className="asb-empty-state">
        <span className="asb-empty-state-icon" aria-hidden="true">
          <RestoreIcon />
        </span>
        <h3 className="asb-section-title">暂无备份</h3>
      </div>
    );
  }

  return (
    <div className="asb-backups">
      <table className="asb-table" aria-label="备份历史">
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.key} scope="col" className="asb-table-cell">
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {records.map((record) => (
            <Fragment key={record.id}>
              <tr>
                {columns.map((column) => (
                  <td key={column.key} className={cx("asb-table-cell", column.cellClassName)}>
                    {column.render(record)}
                  </td>
                ))}
              </tr>
              {openDiffs.has(record.id) && (
                <tr className="asb-backup-diff-row">
                  <td colSpan={columns.length}>
                    <BackupDiff record={record} />
                  </td>
                </tr>
              )}
            </Fragment>
          ))}
        </tbody>
      </table>
      {pending && (
        <ConfirmSheet
          title="恢复备份"
          confirmLabel="确认恢复"
          onConfirm={() => {
            onRestore(pending.id);
            setPending(null);
          }}
          onCancel={() => setPending(null)}
        >
          <ul className="asb-dialog-details">
            <li>时间 <Time iso={pending.createdAt} /></li>
            <li>客户端 {clientLabel(pending.app)}</li>
            <li>内容哈希 {pending.contentHash.slice(0, 12)}</li>
            <li>当前内容会先另行备份，恢复本身可撤销。</li>
          </ul>
        </ConfirmSheet>
      )}
    </div>
  );
}
