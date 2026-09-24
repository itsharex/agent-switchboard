import { useMessageState } from "../i18n/use-message-state";
import { Fragment, useEffect, useState } from "react";
import { backupDiff, type BackupRecord, type KeyChange } from "../api/client";
import { DiffView } from "./DiffView";
import { Button } from "./Button";
import { ConfirmSheet } from "./ConfirmSheet";
import { Pagination } from "./Pagination";
import { type TableColumn } from "./Table";
import { Time } from "./Time";
import { RestoreIcon } from "./icons";
import { useI18n, type MessageKey } from "../i18n";
import { tr } from "../i18n/current";
import { cx } from "@/utils/cx";

interface Props {
  records: BackupRecord[];
  busy: boolean;
  onRestore: (backupId: string) => void;
}

/** The backup table's page size, matching the codebase's page-size convention. */
const BACKUP_PAGE_SIZE = 20;

const REASON_LABEL: Record<string, MessageKey> = {
  "switch": "backup.reason.switch",
  "provider-projection": "backup.reason.providerProjection",
  "restore-precheck": "backup.reason.restorePrecheck",
  "gateway-port-change": "backup.reason.gatewayPortChange",
  "gateway-port-rollback": "backup.reason.gatewayPortRollback",
  "client-configuration-apply": "backup.reason.clientConfigApply",
  "client-configuration-repair": "backup.reason.clientConfigRepair",
  "client-configuration-native-defaults": "backup.reason.nativeDefaults",
  "client-configuration-native-defaults-unmanaged": "backup.reason.nativeDefaultsUnmanaged",
  "client-configuration-clear-extra-configuration": "backup.reason.clearExtraConfiguration",
};

function reasonLabel(reason: string): string {
  const key = REASON_LABEL[reason];
  return key ? tr(key) : reason;
}

function isGatewayPortBackup(reason: string): boolean {
  return reason === "gateway-port-change" || reason === "gateway-port-rollback";
}

function clientLabel(app: string): string {
  return app === "codex" ? "Codex" : "Claude";
}

/** File and saved client preference differences, fetched while the region is open. */
function BackupDiff({ record }: { record: BackupRecord }) {
  const { t } = useI18n();
  const [changes, setChanges] = useState<KeyChange[] | null>(null);
  const [error, setError] = useMessageState();

  useEffect(() => {
    let active = true;
    backupDiff(record.id)
      .then((next) => {
        if (active) setChanges(next);
      })
      .catch((caught) => {
        if (active) setError(caught);
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
  if (changes.length === 0) return <p className="asb-empty">{t("backup.diff.identical")}</p>;
  return <DiffView changes={changes} label={t("backup.diff.label")} />;
}

/** Recent validation and restore history (DESIGN.md §7 bottom band). The
 * table markup mirrors the shared Table contract because an open diff needs
 * a full-width expansion row under its own row, which the shared renderer
 * does not emit. */
export function BackupHistory({ records, busy, onRestore }: Props) {
  const { t } = useI18n();
  const [pending, setPending] = useState<BackupRecord | null>(null);
  const [openDiffs, setOpenDiffs] = useState<ReadonlySet<string>>(() => new Set());
  const [page, setPage] = useState(1);

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

  /** A page turn is a new view context: diffs opened on another page collapse. */
  const turnPage = (next: number) => {
    setPage(next);
    setOpenDiffs(new Set());
  };

  // A restore adds a pre-restore backup, shifting the list; the rendered page
  // converges instead of showing an empty slice.
  const pageCount = Math.max(1, Math.ceil(records.length / BACKUP_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const pageRecords = records.slice(
    (currentPage - 1) * BACKUP_PAGE_SIZE,
    currentPage * BACKUP_PAGE_SIZE,
  );

  const columns: Array<TableColumn<BackupRecord>> = [
    {
      key: "createdAt",
      header: t("backup.col.time"),
      cellClassName: "asb-code",
      render: (record) => <Time iso={record.createdAt} />,
    },
    { key: "app", header: t("backup.col.client"), render: (record) => clientLabel(record.app) },
    { key: "reason", header: t("backup.col.reason"), render: (record) => reasonLabel(record.reason) },
    {
      key: "contentHash",
      header: t("backup.col.hash"),
      cellClassName: "asb-code",
      render: (record) => record.contentHash.slice(0, 12),
    },
    {
      key: "actions",
      header: t("backup.col.actions"),
      render: (record) => {
        const isGatewayPortChange = isGatewayPortBackup(record.reason);
        return (
          <div className="asb-backup-actions">
            {isGatewayPortChange ? (
              <span className="asb-scope-note">{t("backup.history.gatewayPortNote")}</span>
            ) : (
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => setPending(record)}
              >
                {t("backup.history.restore")}
              </Button>
            )}
            <Button
              variant="secondary"
              disabled={busy}
              aria-expanded={openDiffs.has(record.id)}
              onClick={() => toggleDiff(record.id)}
            >
              {t("backup.history.viewDiff")}
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
        <h3 className="asb-section-title">{t("backup.history.empty")}</h3>
      </div>
    );
  }

  return (
    <div className="asb-backups">
      <table className="asb-table" aria-label={t("backup.history.tableAria")}>
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
          {pageRecords.map((record) => (
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
      <Pagination
        total={records.length}
        page={currentPage}
        pageSize={BACKUP_PAGE_SIZE}
        onPageChange={turnPage}
        label={t("backup.history.paginationAria")}
      />
      {pending && (
        <ConfirmSheet
          title={t("backup.history.confirmTitle")}
          confirmLabel={t("backup.confirmRestore")}
          onConfirm={() => {
            onRestore(pending.id);
            setPending(null);
          }}
          onCancel={() => setPending(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("backup.col.time")} <Time iso={pending.createdAt} /></li>
            <li>{t("backup.col.client")} {clientLabel(pending.app)}</li>
            <li>{t("backup.col.hash")} {pending.contentHash.slice(0, 12)}</li>
            <li>{t("backup.history.confirmBackupFirst")}</li>
            <li>{t("backup.history.confirmRestoreSettings")}</li>
          </ul>
        </ConfirmSheet>
      )}
    </div>
  );
}
