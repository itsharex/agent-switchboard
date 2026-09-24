import {
  type CodexOfficialQuotaReset,
} from "../api/client";
import type { MessageKey } from "../i18n";
import { useI18n } from "../i18n";
import { OfficialQuotaTrend } from "./OfficialQuotaTrend";
import { Time } from "./Time";
import { QuotaWindowsTable } from "./QuotaWindowsTable";
import { ModuleHeader } from "./WorkspaceHeader";
import { useCodexOfficialReset } from "./quota-reads";
import { UsageIcon } from "./icons";

const RESET_KIND_LABELS: Record<CodexOfficialQuotaReset["kind"], MessageKey> = {
  early: "codex.officialReset.kindEarly",
  scheduled: "codex.officialReset.kindRoutine",
};

/** An explicit, read-only view of the machine's own Codex official quota
 * reset state. It never reads the public reset-signal feed below it; the
 * refresh action and freshness state live in the usage page header. */
export function CodexOfficialResetPanel({ read }: { read: ReturnType<typeof useCodexOfficialReset> }) {
  const { t } = useI18n();
  const { quota, freshness, cacheLoading, cacheError, readError, statusNotice, history } = read;

  return (
    <section className="asb-panel asb-official-reset" aria-labelledby="codex-official-reset-heading">
      <ModuleHeader id="codex-official-reset-heading" title={t("codex.officialReset.title")} />
      {cacheLoading && quota === null && (
        <p className="asb-empty" role="status">{t("codex.reset.loadingCache")}</p>
      )}
      {quota === null && !cacheLoading && !cacheError && !statusNotice && !readError && (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UsageIcon />
          </span>
          <h3 className="asb-section-title">{t("codex.officialReset.empty")}</h3>
        </div>
      )}
      {cacheError && <p className="asb-warn-text" role="alert">{t("codex.reset.cacheUnavailable", { error: cacheError })}</p>}
      {statusNotice && <p className="asb-warn-text" role="alert">{statusNotice}</p>}
      {readError && (
        <p className="asb-warn-text" role="alert">
          {t("codex.officialReset.readFailed", { error: readError })}
          {quota !== null ? t("codex.reset.showingStale") : ""}
        </p>
      )}
      {quota !== null && (
        <>
          <OfficialQuotaTrend
            series={history.series}
            loading={history.loading}
            error={history.error}
            ariaLabel={t("codex.officialReset.trendAria")}
          />
          <QuotaWindowsTable windows={quota.windows} ariaLabel={t("codex.officialReset.windowsAria")} />
          <p className="asb-official-reset-meta">
            {quota.lastReset ? (
              <>
                {t("codex.officialReset.lastDetected")}{t(RESET_KIND_LABELS[quota.lastReset.kind])} · {t("codex.officialReset.signalTime")}{" "}
                <Time iso={quota.lastReset.observedAt} />
                {quota.lastReset.resetsAt && (
                  <>
                    {" · "}
                    {t("codex.officialReset.newResetTime")}{" "}
                    <Time iso={quota.lastReset.resetsAt} />
                  </>
                )}
              </>
            ) : (
              t("codex.officialReset.noResetDetected")
            )}
          </p>
          {quota.at && (
            <p className="asb-official-reset-meta">
              {freshness === "cached" ? t("codex.reset.cachedAt") : t("codex.officialReset.readAt")} <Time iso={quota.at} />
            </p>
          )}
          <p className="asb-official-reset-note">
            {t("codex.officialReset.note")}
          </p>
        </>
      )}
    </section>
  );
}
