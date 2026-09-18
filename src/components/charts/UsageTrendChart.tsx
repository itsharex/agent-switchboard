import { useId, useMemo, useState } from "react";
import { Area, ComposedChart, Line, Tooltip, XAxis, YAxis } from "recharts";
import { useCountUp } from "@/hooks/use-count-up";
import { ChartFrame } from "./ChartFrame";
import {
  chartSeriesColor,
  formatChartAxisTime,
  formatChartAxisValue,
  formatChartTimestamp,
  formatChartTooltipDate,
  formatChartValue,
  prepareTrendSeries,
  trendYAxisDomain,
  trendSeriesShareUnit,
  type PreparedTrendSeries,
  type UsageTrendSeries,
  type UsageTrendValueKind,
} from "./chart-data";

interface Props {
  series: UsageTrendSeries[];
  ariaLabel: string;
  emptyMessage: string;
  /** Visible heading for the standalone local-usage analysis surface. */
  title?: string;
  /** Source semantics determine formatting and vertical scale. Units alone do
   * not identify a local Token total. */
  valueKind: UsageTrendValueKind;
  /** "default" is the full showcase card for the usage page; "compact" fits
   * trend charts embedded in provider and official-quota panels. */
  size?: "default" | "compact";
}

/** Axis tick text rides the caption step; SVG text needs the literal size. */
const AXIS_TICK = { fontSize: 12, fill: "var(--asb-text-muted)" };
/** The usage trend chart card, fed by real recorded points only. It preserves
 * input point order and never fills missing timestamps with inferred values;
 * series that were read at different times simply leave gaps instead of
 * inventing points. */
export function UsageTrendChart({
  series,
  ariaLabel,
  emptyMessage,
  title,
  valueKind,
  size = "default",
}: Props) {
  const prepared = useMemo(() => prepareTrendSeries(series), [series]);

  if (prepared.length === 0) {
    return <p className="asb-chart-empty" role="status">{emptyMessage || "暂无可用趋势数据。"}</p>;
  }

  if (!trendSeriesShareUnit(prepared)) {
    return <p className="asb-chart-empty" role="alert">无法将不同单位的数据放在同一趋势图中。</p>;
  }

  return (
    <TrendCard
      prepared={prepared}
      valueKind={valueKind}
      ariaLabel={ariaLabel}
      title={title}
      size={size}
    />
  );
}

function TrendCard({
  prepared,
  valueKind,
  ariaLabel,
  title,
  size,
}: {
  prepared: PreparedTrendSeries[];
  valueKind: UsageTrendValueKind;
  ariaLabel: string;
  title?: string;
  size: "default" | "compact";
}) {
  const gradientId = useId();
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const compact = size === "compact";

  const rows = mergeSeriesRows(prepared);
  const unit = prepared[0]?.unit ?? null;
  const pointCount = prepared.reduce((total, entry) => total + entry.points.length, 0);

  // Rest state and hover state both resolve to a real recorded point. A sum
  // is available only for local-token series when every series recorded the
  // same timestamp.
  const activeRow = activeIndex !== null && activeIndex < rows.length ? rows[activeIndex] : null;
  const figure = resolveTrendFigure(rows, prepared, activeRow, valueKind === "local-token");
  const display = useCountUp(figure.value);

  const label = activeRow ? formatChartAxisTime(figure.timestamp) : figure.label;
  const caption = activeRow
    ? `${figure.label} · ${formatChartTimestamp(figure.timestamp)}`
    : unit
      ? `单位：${unit}`
      : `${pointCount} 个真实读数`;

  return (
    <figure
      className={compact ? "asb-usage-chart-card is-compact" : "asb-usage-chart-card"}
      aria-label={ariaLabel}
    >
      {title && <figcaption className="asb-usage-chart-title">{title}</figcaption>}
      {/* Header: label over the count-up figure and its caption; legend on the right */}
      <div className="asb-usage-chart-head">
        <div className="asb-usage-chart-readout">
          <p className="asb-usage-chart-label">{label}</p>
          <p
            key={activeIndex ?? "rest"}
            className="asb-usage-chart-value"
          >
            {formatChartValue(display, unit, valueKind)}
          </p>
          <p className="asb-usage-chart-caption">{caption}</p>
        </div>
        <dl className="asb-usage-chart-legend">
          {prepared.map((entry, index) => (
            <div key={entry.id} className="asb-usage-chart-legend-item">
              <span
                className="asb-usage-chart-swatch"
                style={{ backgroundColor: chartSeriesColor(index) }}
                aria-hidden
              />
              <dt className="asb-usage-chart-legend-label">{entry.label}</dt>
            </div>
          ))}
        </dl>
      </div>

      {/* Chart */}
      <div className="asb-usage-chart-body">
        <ChartFrame>
          <ComposedChart
            data={rows}
            margin={{ top: 4, right: 6, bottom: 0, left: 0 }}
            onMouseMove={(nextState) => {
              const index = Number(nextState.activeTooltipIndex);
              if (nextState.isTooltipActive && Number.isFinite(index)) setActiveIndex(index);
            }}
            onMouseLeave={() => setActiveIndex(null)}
          >
            <defs>
              <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={chartSeriesColor(0)} stopOpacity={0.35} />
                <stop offset="100%" stopColor={chartSeriesColor(0)} stopOpacity={0} />
              </linearGradient>
            </defs>
            <YAxis
              width={44}
              domain={trendYAxisDomain(prepared, valueKind)}
              tickCount={4}
              tickFormatter={(value: number) => formatChartAxisValue(value, valueKind)}
              tickLine={false}
              axisLine={false}
              tick={AXIS_TICK}
            />
            <XAxis
              dataKey="timestamp"
              type="number"
              scale="time"
              domain={["dataMin", "dataMax"]}
              tickFormatter={(value: number) => formatChartAxisTime(Number(value))}
              tickLine={false}
              axisLine={false}
              tickMargin={12}
              tick={AXIS_TICK}
            />
            <Tooltip
              isAnimationActive={false}
              content={({ active, payload }) => {
                if (!active || payload.length === 0) return null;
                const row = payload[0]?.payload as TrendRow | undefined;
                if (!row) return null;
                return (
                  <TrendTooltipCard
                    row={row}
                    prepared={prepared}
                    unit={unit}
                    valueKind={valueKind}
                  />
                );
              }}
              cursor={{ stroke: "var(--asb-hairline-strong)", strokeWidth: 1, strokeDasharray: "4 4" }}
            />
            {prepared.length === 1 && (
              <Area
                type="monotone"
                dataKey={prepared[0].id}
                stroke="none"
                fill={`url(#${gradientId})`}
                dot={false}
                connectNulls
                isAnimationActive={false}
              />
            )}
            {prepared.map((entry, index) => (
              <Line
                key={entry.id}
                type="monotone"
                dataKey={entry.id}
                name={entry.label}
                stroke={chartSeriesColor(index)}
                strokeWidth={2.5}
                // A lone real point draws no line; show it as a dot instead of
                // leaving the only recorded value invisible.
                dot={entry.points.length === 1 ? { r: 3 } : false}
                activeDot={<ActiveDot color={chartSeriesColor(index)} />}
                connectNulls
                isAnimationActive={false}
              />
            ))}
          </ComposedChart>
        </ChartFrame>
      </div>
    </figure>
  );
}

/** The hover marker: a soft halo behind a solid dot ringed by the card
 * surface. Recharts clones this element with the active point's coordinates. */
function ActiveDot({ color, cx, cy }: { color: string; cx?: number; cy?: number }) {
  if (cx === undefined || cy === undefined) return null;
  return (
    <g>
      <circle cx={cx} cy={cy} r={7} fill={color} opacity={0.25} />
      <circle
        cx={cx}
        cy={cy}
        r={4}
        fill={color}
        stroke="var(--asb-content-muted)"
        strokeWidth={2}
      />
    </g>
  );
}

/** The hover tooltip: a floating card restating the hovered column as one row
 * per series that actually recorded a point there. Series without a real
 * value are omitted, mirroring the chart's no-invented-values rule; a sum is
 * shown only for local-token trends, matching the card header. */
function TrendTooltipCard({
  row,
  prepared,
  unit,
  valueKind,
}: {
  row: TrendRow;
  prepared: PreparedTrendSeries[];
  unit: string | null;
  valueKind: UsageTrendValueKind;
}) {
  const entries = prepared
    .map((entry, index) => ({ entry, index, value: row[entry.id] }))
    .filter((item) => typeof item.value === "number");
  if (entries.length === 0) return null;

  const total =
    valueKind === "local-token" ? entries.reduce((sum, item) => sum + item.value!, 0) : null;

  return (
    <div className="asb-usage-chart-tooltip">
      <p className="asb-usage-chart-tooltip-title">
        {formatChartTooltipDate(row.timestamp)}
        {total !== null ? ` · ${formatChartValue(total, unit, valueKind)}` : null}
      </p>
      <div className="asb-usage-chart-tooltip-rows">
        {entries.map(({ entry, index, value }) => (
          <div key={entry.id} className="asb-usage-chart-tooltip-row">
            <span className="asb-usage-chart-tooltip-series">
              <span
                className="asb-usage-chart-swatch"
                style={{ backgroundColor: chartSeriesColor(index) }}
                aria-hidden
              />
              {entry.label}
            </span>
            <span className="asb-usage-chart-tooltip-value">
              {formatChartValue(value!, unit, valueKind)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

interface TrendRow {
  timestamp: number;
  [seriesId: string]: number | undefined;
}

interface TrendFigure {
  label: string;
  value: number;
  timestamp: number;
}

function resolveTrendFigure(
  rows: TrendRow[],
  series: PreparedTrendSeries[],
  activeRow: TrendRow | null,
  aggregateSeries: boolean,
): TrendFigure {
  if (activeRow) return figureForRow(activeRow, series, aggregateSeries);

  if (aggregateSeries) {
    const latestCompleteRow = [...rows].reverse().find((row) => rowHasEverySeries(row, series));
    if (latestCompleteRow) return aggregateFigure(latestCompleteRow, series);
  }

  return latestObservedFigure(series);
}

function figureForRow(
  row: TrendRow,
  series: PreparedTrendSeries[],
  aggregateSeries: boolean,
): TrendFigure {
  if (aggregateSeries && rowHasEverySeries(row, series)) return aggregateFigure(row, series);

  const observedSeries = series.find((entry) => typeof row[entry.id] === "number");
  if (!observedSeries) return latestObservedFigure(series);

  return {
    label: observedSeries.label,
    value: row[observedSeries.id]!,
    timestamp: row.timestamp,
  };
}

function aggregateFigure(row: TrendRow, series: PreparedTrendSeries[]): TrendFigure {
  return {
    label: "合计",
    value: series.reduce((sum, entry) => sum + row[entry.id]!, 0),
    timestamp: row.timestamp,
  };
}

function latestObservedFigure(series: PreparedTrendSeries[]): TrendFigure {
  return series
    .flatMap((entry) => entry.points.map((point) => ({ label: entry.label, value: point.value, timestamp: point.timestamp })))
    .reduce((latest, point) => (point.timestamp > latest.timestamp ? point : latest));
}

function rowHasEverySeries(row: TrendRow, series: PreparedTrendSeries[]): boolean {
  return series.every((entry) => typeof row[entry.id] === "number");
}

/** Union of every real timestamp; each series contributes only the points it
 * actually recorded, so nothing is interpolated into the gaps. */
function mergeSeriesRows(series: PreparedTrendSeries[]): TrendRow[] {
  const timestamps = new Set<number>();
  for (const entry of series) {
    for (const point of entry.points) timestamps.add(point.timestamp);
  }

  const pointsById = new Map(
    series.map((entry) => [entry.id, new Map(entry.points.map((point) => [point.timestamp, point.value]))]),
  );

  return [...timestamps]
    .sort((left, right) => left - right)
    .map((timestamp) => {
      const row: TrendRow = { timestamp };
      for (const [id, points] of pointsById) {
        const value = points.get(timestamp);
        if (value !== undefined) row[id] = value;
      }
      return row;
    });
}
