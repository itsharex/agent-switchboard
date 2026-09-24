import { officialQuotaWindowName } from "../lib/usage-format";
import type { UsageHistorySeries } from "../api/client";
import { useI18n } from "../i18n";
import { UsageTrendChart } from "./charts/UsageTrendChart";

interface Props {
  series: UsageHistorySeries[];
  loading: boolean;
  error: string | null;
  ariaLabel: string;
}

/** One account-level official quota trend. Only normalized successful reads
 * enter this series; a stale or failed response never adds a point. */
export function OfficialQuotaTrend({ series, loading, error, ariaLabel }: Props) {
  const { t } = useI18n();
  const quotaSeries = series
    .filter((entry) => entry.metric === "usedPercent" && entry.points.length > 0)
    .map((entry) => ({
      id: entry.id,
      label: officialQuotaWindowName(entry.label),
      unit: entry.unit ?? "%",
      points: entry.points,
    }));

  return (
    <section className="asb-usage-history" aria-label={ariaLabel}>
      <h4 className="asb-section-title">{t("clientConfig.quota.trendTitle")}</h4>
      {loading && quotaSeries.length === 0 ? (
        <span className="asb-skeleton asb-usage-history-skeleton" role="status" aria-label={t("clientConfig.quota.historyLoading")} />
      ) : (
        <UsageTrendChart
          size="compact"
          series={quotaSeries}
          ariaLabel={ariaLabel}
          emptyMessage={t("clientConfig.quota.trendEmpty")}
          valueKind="percentage"
        />
      )}
      {error && <p className="asb-warn-text" role="alert">{t("clientConfig.quota.historyError", { error })}</p>}
    </section>
  );
}
