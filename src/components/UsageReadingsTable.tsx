import type { UsageReading } from "../api/client";
import { formatUsageValue, usageProgress, usageWindowName } from "../lib/usage-format";
import { UsageRatioMeter } from "./charts/UsageRatioMeter";
import { Table, type TableColumn } from "./Table";
import { Time } from "./Time";

/** One reading as a table row: plan identity plus its three measures. */
interface UsageRow {
  key: string;
  name: string;
  reading: UsageReading;
}

const USAGE_COLUMNS: Array<TableColumn<UsageRow>> = [
  { key: "plan", header: "额度", render: (row) => row.name },
  {
    key: "remaining",
    header: "余额",
    render: (row) => formatUsageValue(row.reading.remaining, row.reading.unit),
  },
  {
    key: "used",
    header: "已用",
    render: (row) => formatUsageValue(row.reading.used, row.reading.unit),
  },
  {
    key: "total",
    header: "总量",
    render: (row) => formatUsageValue(row.reading.total, row.reading.unit),
  },
  {
    key: "ratio",
    header: "占比",
    render: (row) => {
      const progress = row.reading.isValid === false ? null : usageProgress(row.reading);
      return (
        <UsageRatioMeter
          percent={progress}
          ariaLabel={`${row.name} 已用比例`}
        />
      );
    },
  },
  { key: "reset", header: "重置时间", render: ({ reading }) => reading.resetsAt ? <Time iso={reading.resetsAt} /> : "—" },
  { key: "status", header: "状态 / 说明", render: ({ reading }) => <>
    {reading.isValid === false && <span className="asb-usage-invalid">{reading.invalidMessage || "已失效"}</span>}
    {reading.isValid === true && <span>有效</span>}
    {reading.extra && <span className="asb-usage-note">{reading.extra}</span>}
    {reading.isValid === undefined && !reading.extra && "—"}
  </> },
];

interface Props {
  readings: UsageReading[];
  ariaLabel: string;
}

/** The single usage-readings table: every usage module renders its readings
 * through this owner so the measure columns stay one contract. */
export function UsageReadingsTable({ readings, ariaLabel }: Props) {
  return (
    <Table
      columns={USAGE_COLUMNS}
      rows={readings.map((reading, index) => ({
        key: `${reading.planName ?? "默认"}-${index}`,
        name: reading.planName?.trim() ? usageWindowName(reading.planName.trim()) : "默认额度",
        reading,
      }))}
      rowKey={(row) => row.key}
      ariaLabel={ariaLabel}
    />
  );
}
