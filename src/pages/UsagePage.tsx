import {
  type ModelUsageGroup,
  type ModelUsageReport,
} from "../api/client";
import {
  ModelUsageDistributionChart,
} from "../components/charts/ModelUsageDistributionChart";
import { UsageTrendChart } from "../components/charts/UsageTrendChart";
import { StatCards } from "../components/charts/stat-cards";
import { Table, type TableColumn } from "../components/Table";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { UsageIcon } from "../components/icons";
import { useI18n, type TFunction } from "../i18n";
import { clientName } from "../lib/client-name";
import { TOKEN_UNIT, formatCompactTokenCount, formatTokenCount } from "../lib/token-format";
import { useModelUsageReport } from "./use-model-usage-report";

function cachedTokenCount(group: ModelUsageGroup): number {
  return group.cacheReadInputTokens + group.cacheCreationInputTokens;
}

function reportTotal(report: ModelUsageReport, selector: (group: ModelUsageGroup) => number): number {
  return report.groups.reduce((total, group) => total + selector(group), 0);
}

function dailyTrend(report: ModelUsageReport, t: TFunction) {
  return [
    {
      id: "fresh-input",
      label: t("usage.label.freshInput"),
      unit: TOKEN_UNIT,
      points: report.days.map((day) => ({ at: `${day.date}T12:00:00`, value: day.inputTokens })),
    },
    {
      id: "cache",
      label: t("usage.label.cache"),
      unit: TOKEN_UNIT,
      points: report.days.map((day) => ({
        at: `${day.date}T12:00:00`,
        value: day.cacheReadInputTokens + day.cacheCreationInputTokens,
      })),
    },
    {
      id: "output",
      label: t("usage.label.output"),
      unit: TOKEN_UNIT,
      points: report.days.map((day) => ({ at: `${day.date}T12:00:00`, value: day.outputTokens })),
    },
  ];
}

function modelComposition(report: ModelUsageReport, t: TFunction) {
  return report.groups.map((group, index) => ({
    id: `${group.app}-${group.model ?? "unknown"}-${index}`,
    label: `${clientName(group.app)} · ${group.model ?? t("usage.model.unrecorded")}`,
    value: group.totalTokens,
  }));
}

function usageColumns(t: TFunction): Array<TableColumn<ModelUsageGroup>> {
  return [
    {
      key: "app",
      header: t("usage.column.client"),
      render: (group) => clientName(group.app),
    },
    {
      key: "model",
      header: t("usage.column.model"),
      cellClassName: "asb-code",
      render: (group) => group.model ?? t("usage.model.unrecorded"),
    },
    {
      key: "input",
      header: t("usage.column.input"),
      render: (group) => formatTokenCount(group.inputTokens),
    },
    {
      key: "cache",
      header: t("usage.column.cache"),
      render: (group) => formatTokenCount(cachedTokenCount(group)),
    },
    {
      key: "output",
      header: t("usage.column.output"),
      render: (group) => formatTokenCount(group.outputTokens),
    },
    {
      key: "total",
      header: t("usage.column.totalTokens"),
      render: (group) => formatTokenCount(group.totalTokens),
    },
    {
      key: "sessions",
      header: t("usage.column.sessions"),
      render: (group) => formatTokenCount(group.sessionCount),
    },
  ];
}

/** Read-only local session token totals. Provider quota remains in provider panels.
 * Range selection, refresh and the snapshot status line live in the page header;
 * the card renders the report it receives. */
export function UsagePage({ usage }: {
  usage: ReturnType<typeof useModelUsageReport>;
}) {
  const { t } = useI18n();
  const { read, loading, error } = usage;
  const report = read?.report ?? null;

  const freshInput = report ? reportTotal(report, (group) => group.inputTokens) : 0;
  const cachedInput = report ? reportTotal(report, cachedTokenCount) : 0;
  const output = report ? reportTotal(report, (group) => group.outputTokens) : 0;
  const total = report ? reportTotal(report, (group) => group.totalTokens) : 0;
  const undated = report?.unassignedTokens.totalTokens ?? 0;

  return (
    <section className="asb-panel asb-model-usage" aria-label={t("usage.consumption")}>
      <ModuleHeader title={t("usage.consumption")} />
      {error && <p className="asb-model-usage-notice" role="alert">{error}</p>}
      {read?.cacheWarning && <p className="asb-warn-text" role="alert">{read.cacheWarning}</p>}
      {report?.issues.length ? (
        <ul className="asb-model-usage-issues" aria-label={t("usage.issues.aria")}>
          {report.issues.map((issue) => (
            <li key={`${issue.app}-${issue.message}`} className="asb-warn-text">
              {t("usage.issues.entry", { client: clientName(issue.app), message: issue.message })}
            </li>
          ))}
        </ul>
      ) : null}
      {loading && report === null ? (
        <p className="asb-empty asb-model-usage-empty" role="status">
          {t("usage.loading")}
        </p>
      ) : report?.groups.length ? (
        <div className="asb-model-usage-content">
          <div className="asb-model-usage-summary">
            <div role="group" aria-label={t("usage.summary.groupAria")}>
              <StatCards
                stats={[
                  { label: t("usage.label.total"), value: formatCompactTokenCount(total), unit: TOKEN_UNIT },
                  { label: t("usage.label.freshInput"), value: formatCompactTokenCount(freshInput), unit: TOKEN_UNIT },
                  { label: t("usage.label.cache"), value: formatCompactTokenCount(cachedInput), unit: TOKEN_UNIT },
                  { label: t("usage.label.output"), value: formatCompactTokenCount(output), unit: TOKEN_UNIT },
                ]}
              />
            </div>
            {undated > 0 && (
              <p className="asb-model-usage-undated" role="status">
                {t("usage.undated", { count: formatTokenCount(undated) })}
              </p>
            )}
          </div>
          <div className="asb-model-usage-analysis">
            <section className="asb-model-usage-distribution" aria-label={t("usage.composition.title")}>
              <ModelUsageDistributionChart
                items={modelComposition(report, t)}
                ariaLabel={t("usage.composition.chartAria")}
                emptyMessage={t("usage.composition.empty")}
              />
            </section>
            <section className="asb-model-usage-trend" aria-label={t("usage.trend.title")}>
              <UsageTrendChart
                series={dailyTrend(report, t)}
                ariaLabel={t("usage.trend.chartAria")}
                title={t("usage.trend.title")}
                emptyMessage={t("usage.trend.empty")}
                valueKind="local-token"
              />
            </section>
          </div>
          <div className="asb-model-usage-table-wrap">
            <Table
              columns={usageColumns(t)}
              rows={report.groups}
              rowKey={(group, index) => `${group.app}-${group.model ?? "unknown"}-${index}`}
              ariaLabel={t("usage.consumption")}
              className="asb-model-usage-table"
            />
          </div>
        </div>
      ) : report ? (
        <div className="asb-empty-state asb-model-usage-empty">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UsageIcon />
          </span>
          <h3 className="asb-section-title">{t("usage.empty")}</h3>
        </div>
      ) : null}
    </section>
  );
}
