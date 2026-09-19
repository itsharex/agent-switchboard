import {
  type CodexOfficialQuotaReset,
} from "../api/client";
import { OfficialQuotaTrend } from "./OfficialQuotaTrend";
import { Time } from "./Time";
import { QuotaWindowsTable } from "./QuotaWindowsTable";
import { ModuleHeader } from "./WorkspaceHeader";
import { useCodexOfficialReset } from "./quota-reads";
import { UsageIcon } from "./icons";

function resetKindLabel(kind: CodexOfficialQuotaReset["kind"]): string {
  return kind === "early" ? "提前重置" : "例行重置";
}

/** An explicit, read-only view of the machine's own Codex official quota
 * reset state. It never reads the public reset-signal feed below it; the
 * refresh action and freshness state live in the usage page header. */
export function CodexOfficialResetPanel({ read }: { read: ReturnType<typeof useCodexOfficialReset> }) {
  const { quota, freshness, cacheLoading, cacheError, readError, statusNotice, history } = read;

  return (
    <section className="asb-panel asb-official-reset" aria-labelledby="codex-official-reset-heading">
      <ModuleHeader id="codex-official-reset-heading" title="Codex 官方额度重置" />
      {cacheLoading && quota === null && (
        <p className="asb-empty" role="status">正在读取本地缓存</p>
      )}
      {quota === null && !cacheLoading && !cacheError && !statusNotice && !readError && (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UsageIcon />
          </span>
          <h3 className="asb-section-title">尚无官方额度读取记录。手动刷新以读取本机 Codex 官方登录。</h3>
        </div>
      )}
      {cacheError && <p className="asb-warn-text" role="alert">本地缓存不可用：{cacheError}</p>}
      {statusNotice && <p className="asb-warn-text" role="alert">{statusNotice}</p>}
      {readError && (
        <p className="asb-warn-text" role="alert">
          无法刷新官方额度：{readError}
          {quota !== null ? "；仍在显示上次成功读取的数据。" : ""}
        </p>
      )}
      {quota !== null && (
        <>
          <OfficialQuotaTrend
            series={history.series}
            loading={history.loading}
            error={history.error}
            ariaLabel="Codex 官方额度趋势"
          />
          <QuotaWindowsTable windows={quota.windows} ariaLabel="Codex 官方额度窗口" />
          <p className="asb-official-reset-meta">
            {quota.lastReset ? (
              <>
                上次检测到重置：{resetKindLabel(quota.lastReset.kind)} · 信号时间{" "}
                <Time iso={quota.lastReset.observedAt} />
                {quota.lastReset.resetsAt && (
                  <>
                    {" · 新重置时间 "}
                    <Time iso={quota.lastReset.resetsAt} />
                  </>
                )}
              </>
            ) : (
              "尚未检测到重置（每次成功读取自动比对）"
            )}
          </p>
          {quota.at && (
            <p className="asb-official-reset-meta">
              {freshness === "cached" ? "缓存于" : "读取于"} <Time iso={quota.at} />
            </p>
          )}
          <p className="asb-official-reset-note">
            数据来自本机 Codex 官方登录，仅代表当前账号额度，与上方公开重置信号相互独立。
          </p>
        </>
      )}
    </section>
  );
}
