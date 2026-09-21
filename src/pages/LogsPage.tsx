import {
  type RuntimeLogAction,
  type RuntimeLogEntry,
} from "../api/client";
import { Pagination } from "../components/Pagination";
import { Table, type TableColumn } from "../components/Table";
import { Time } from "../components/Time";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { ScrollTextIcon } from "../components/icons";
import { RUNTIME_LOG_PAGE_SIZE, levelLabel, useRuntimeLogs } from "./use-runtime-logs";

const ACTION_LABEL: Record<RuntimeLogAction, string> = {
  appStarted: "应用已启动",
  appSettingsSaved: "已保存应用设置",
  appSettingsRepaired: "已修复应用设置",
  profileStoreReset: "已重置供应商数据",
  profileCreated: "已创建供应商档案",
  profileUpdated: "已更新供应商档案",
  profileDeleted: "已删除供应商档案",
  profilesReordered: "已调整供应商顺序",
  profileImported: "已导入本机供应商档案",
  globalPromptDocumentSaved: "已保存全局提示词文档",
  configurationSwitched: "已切换配置",
  backupRestored: "已恢复备份",
  switchUndone: "已撤回上一次切换",
  staleLockRecovered: "已恢复遗留锁",
  cloudBackupSettingsSaved: "已保存云端备份设置",
  cloudBackupConnectionTested: "已验证云端备份连接",
  cloudBackupUploaded: "已上传云端备份",
  cloudBackupRestored: "已恢复云端备份",
  sessionResumed: "已恢复会话",
  sessionDeleted: "已删除会话",
  ccSwitchProfilesImported: "已导入本机档案",
  officialLoginCompleted: "已完成官方登录",
};

const LOG_COLUMNS: Array<TableColumn<RuntimeLogEntry>> = [
  {
    key: "at",
    header: "时间",
    cellClassName: "asb-code",
    render: (entry) => <Time iso={entry.at} />,
  },
  {
    key: "level",
    header: "级别",
    render: (entry) => (
      <span className={`asb-runtime-log-level is-${entry.level}`}>{levelLabel(entry.level)}</span>
    ),
  },
  {
    key: "action",
    header: "事件",
    cellClassName: "asb-runtime-log-action",
    render: (entry) => ACTION_LABEL[entry.action],
  },
  {
    key: "errorCode",
    header: "错误代码",
    cellClassName: "asb-code",
    render: (entry) => entry.errorCode ?? "—",
  },
];

/** Read-only view of the application's own bounded diagnostic event files.
 * The record-level setting, level filter and refresh actions live in the
 * diagnostics page header; the card renders the current page of entries. */
export function LogsPage({ logs }: { logs: ReturnType<typeof useRuntimeLogs> }) {
  const { entries, visibleEntries, pageEntries, page, setPage, filter, loading, error, folderError } = logs;
  return (
    <section className="asb-panel asb-runtime-logs" aria-labelledby="runtime-logs-heading">
      <ModuleHeader id="runtime-logs-heading" title="日志" />
      {error && (
        <p className="asb-runtime-log-notice" role="alert">
          无法读取应用日志：{error.message}
        </p>
      )}
      {folderError && (
        <p className="asb-runtime-log-notice" role="alert">
          无法打开日志文件夹：{folderError.message}
        </p>
      )}
      {loading && entries.length === 0 ? (
        <div className="asb-runtime-log-table-wrap">
          <div className="asb-runtime-log-skeleton" role="status" aria-label="正在读取应用日志…">
            {Array.from({ length: RUNTIME_LOG_PAGE_SIZE }, (_, index) => (
              <span key={index} className="asb-skeleton asb-runtime-log-skeleton-row" />
            ))}
          </div>
        </div>
      ) : visibleEntries.length === 0 ? (
        <div className="asb-empty-state asb-runtime-log-empty">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <ScrollTextIcon />
          </span>
          <h3 className="asb-section-title">
            {filter === "all" ? "暂无应用运行日志" : `暂无${levelLabel(filter)}级别的应用日志`}
          </h3>
        </div>
      ) : (
        <>
          <div className="asb-runtime-log-table-wrap">
            <Table
              columns={LOG_COLUMNS}
              rows={pageEntries}
              rowKey={(entry, index) => `${entry.at}-${entry.action}-${entry.errorCode ?? ""}-${index}`}
              ariaLabel="应用运行日志"
              className="asb-runtime-log-table"
            />
          </div>
          <Pagination
            total={visibleEntries.length}
            page={page}
            pageSize={RUNTIME_LOG_PAGE_SIZE}
            onPageChange={setPage}
            label="应用运行日志分页"
          />
        </>
      )}
    </section>
  );
}
