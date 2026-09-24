import type { CodexOfficialQuotaWindow } from "../api/client";
import { useI18n, type TFunction } from "../i18n";
import { formatUsageValue, officialQuotaWindowName } from "../lib/usage-format";
import { UsageRatioMeter } from "./charts/UsageRatioMeter";
import { countdownLabel } from "../lib/time";
import { Table, type TableColumn } from "./Table";
import { Time } from "./Time";

function percent(usedPercent: number): string {
  return formatUsageValue(usedPercent, "%");
}

function windowColumns(t: TFunction): Array<TableColumn<CodexOfficialQuotaWindow>> {
  return [
    { key: "label", header: t("usage.windows.column.window"), render: (window) => officialQuotaWindowName(window.label) },
    { key: "used", header: t("usage.column.used"), render: (window) => percent(window.usedPercent) },
    {
      key: "reset",
      header: t("usage.windows.column.reset"),
      render: (window) =>
        window.resetsAt === null ? (
          "—"
        ) : (
          <>
            <Time iso={window.resetsAt} /> · {countdownLabel(window.resetsAt)}
          </>
        ),
    },
    {
      key: "ratio",
      header: t("usage.column.ratio"),
      render: (window) => <UsageRatioMeter percent={window.usedPercent} ariaLabel={t("usage.ratioAria", { name: officialQuotaWindowName(window.label) })} />,
    },
  ];
}

interface Props {
  windows: CodexOfficialQuotaWindow[];
  ariaLabel: string;
}

/** The single official-quota windows table: the provider-card quota panel and
 * the settings reset panel render their server windows through this owner. */
export function QuotaWindowsTable({ windows, ariaLabel }: Props) {
  const { t } = useI18n();
  return (
    <Table
      columns={windowColumns(t)}
      rows={windows}
      rowKey={(window, index) => `${window.label}-${index}`}
      ariaLabel={ariaLabel}
    />
  );
}
