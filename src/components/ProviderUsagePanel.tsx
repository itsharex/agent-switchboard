import type { ProviderProfile } from "../api/client";
import { Button } from "./Button";
import { Time } from "./Time";
import { UsageReadingsTable } from "./UsageReadingsTable";
import type { ProviderUsage } from "./use-provider-usage";

interface Props {
  id: string;
  profile: ProviderProfile;
  usage: ProviderUsage;
  onConfigure?: (profile: ProviderProfile) => void;
}

/** The containing row mounts this panel only while its usage disclosure is open. */
export function ProviderUsagePanel({ id, profile, usage, onConfigure }: Props) {
  const { data: summary, querying, error, run } = usage;

  return (
    <section id={id} className="asb-provider-usage" aria-label={`${profile.name} 用量`}>
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
              onClick={() => onConfigure(profile)}
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
        <UsageReadingsTable readings={summary.readings} ariaLabel={`${profile.name} 用量读数`} />
      ) : (
        !error && <p className="asb-provider-usage-state" role="status">正在读取已配置的用量…</p>
      )}
      {error && <p className="asb-warn-text" role="alert">{error}</p>}
    </section>
  );
}
