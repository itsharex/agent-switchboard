import { Cell, Pie, PieChart } from "recharts";
import { useCountUp } from "@/hooks/use-count-up";
import { TOKEN_UNIT, formatCompactTokenCount, formatTokenValue } from "../../lib/token-format";
import { formatUsageValue } from "../../lib/usage-format";
import { ChartFrame } from "./ChartFrame";
import { chartSeriesColor, type ModelUsageDistributionItem } from "./chart-data";

const TOP_MODEL_COUNT = 5;

type DisplayItem = ModelUsageDistributionItem & {
  otherCount: number | null;
};

interface Props {
  items: ModelUsageDistributionItem[];
  ariaLabel: string;
  emptyMessage: string;
}

/** A model ledger rendered as one prominent ring with an adjacent, auditable
 * legend. Only the tail is combined, and its label records that fact. */
export function ModelUsageDistributionChart({ items, ariaLabel, emptyMessage }: Props) {
  const visible = resolveDisplayItems(items);
  if (visible.length === 0) {
    return <p className="asb-chart-empty" role="status">{emptyMessage || "暂无可用模型构成数据。"}</p>;
  }

  return <DonutCard visible={visible} ariaLabel={ariaLabel} />;
}

function DonutCard({ visible, ariaLabel }: { visible: DisplayItem[]; ariaLabel: string }) {
  const total = visible.reduce((sum, item) => sum + item.value, 0);
  const display = useCountUp(Math.round(total));
  return (
    <figure className="asb-usage-donut-card" aria-label={ariaLabel}>
      <figcaption className="asb-usage-donut-title">模型构成</figcaption>
      <div className="asb-usage-donut-layout">
        <div className="asb-usage-donut-stage">
          <ChartFrame>
            <PieChart>
              <Pie
                data={visible}
                dataKey="value"
                nameKey="label"
                innerRadius="61%"
                outerRadius="90%"
                paddingAngle={1}
                // Slice separators stay the card surface so the ring reads as
                // one shape cut into shares.
                stroke="var(--asb-content-muted)"
                strokeWidth={4}
                isAnimationActive={false}
              >
                {visible.map((item, index) => (
                  <Cell
                    key={item.id}
                    fill={
                      item.otherCount === null
                        ? chartSeriesColor(index)
                        : "var(--asb-hairline-strong)"
                    }
                  />
                ))}
              </Pie>
            </PieChart>
          </ChartFrame>
          <div className="asb-usage-donut-center">
            <span className="asb-usage-donut-center-value">
              {formatCompactTokenCount(display)}
            </span>
            <span className="asb-usage-donut-center-unit">{TOKEN_UNIT}</span>
          </div>
        </div>
        <ol className="asb-usage-donut-legend">
          {visible.map((item, index) => {
            const percent = (item.value / total) * 100;
            const label = item.otherCount === null ? item.label : `其他（${item.otherCount} 个模型）`;
            const color =
              item.otherCount === null ? chartSeriesColor(index) : "var(--asb-hairline-strong)";
            return (
              <li key={item.id} className="asb-usage-donut-row">
                <div className="asb-usage-donut-row-head">
                  <span className="asb-usage-donut-swatch" style={{ backgroundColor: color }} aria-hidden />
                  <span className="asb-usage-donut-name" title={label}>
                    {label}
                  </span>
                  <span className="asb-usage-donut-percent">{formatUsageValue(percent, "%")}</span>
                </div>
                <p className="asb-usage-donut-value">{formatTokenValue(item.value)}</p>
              </li>
            );
          })}
        </ol>
      </div>
    </figure>
  );
}

function resolveDisplayItems(items: ModelUsageDistributionItem[]): DisplayItem[] {
  const realItems = items
    .filter((item) => Number.isFinite(item.value) && item.value > 0)
    .map((item, index) => ({
      ...item,
      id: item.id.trim() || `model-${index}`,
      label: item.label.trim() || "未记录模型",
      otherCount: null,
    }))
    .sort((left, right) => right.value - left.value);
  const leading = realItems.slice(0, TOP_MODEL_COUNT);
  const remainder = realItems.slice(TOP_MODEL_COUNT);
  if (remainder.length === 0) return leading;

  return [
    ...leading,
    {
      id: "model-usage-other",
      label: "其他",
      value: remainder.reduce((sum, item) => sum + item.value, 0),
      otherCount: remainder.length,
    },
  ];
}
