import { Button } from "./Button";
import { Time } from "./Time";
import { UsageReadingsTable } from "./UsageReadingsTable";
import type { ProviderUsage } from "./use-provider-usage";

interface Props {
  id: string;
  name: string;
  usage: ProviderUsage;
  /** Opens the usage-query workspace for this provider. */
  onConfigure?: () => void;
}

/** The containing row mounts this panel only while its usage disclosure is open. */
export function ProviderUsagePanel({ id, name, usage, onConfigure }: Props) {
  const { data: summary, querying, error, run } = usage;

  return (
    <section id={id} className="asb-provider-usage" aria-label={`${name} 用量`}>
      <header className="asb-provider-usage-head">
        <div className="asb-provider-usage-title">
          <h3 className="asb-section-title">用量</h3>
        </div>
        <div className="asb-provider-usage-actions">
          {summary && <Time iso={summary.at} />}
          {onConfigure && (
            <Button
              variant="unstyled"
              className="asb-provider-usage-configure"
              onClick={onConfigure}
            >
              编辑查询
            </Button>
          )}
          <Button
            variant="unstyled"
            className="asb-provider-usage-refresh"
            disabled={querying}
            onClick={() => void run()}
          >
            {querying ? "读取中…" : "刷新"}
          </Button>
        </div>
      </header>

      {summary ? (
        <UsageReadingsTable readings={summary.readings} ariaLabel={`${name} 用量读数`} />
      ) : (
        !error && <p className="asb-provider-usage-state" role="status">{querying ? "正在读取用量…" : "暂无用量读数，可点击刷新查询。"}</p>
      )}
      {error && <p className="asb-warn-text" role="alert">{error}</p>}
    </section>
  );
}
