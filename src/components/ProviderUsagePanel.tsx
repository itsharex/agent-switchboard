import { useI18n } from "../i18n";
import { Button } from "./Button";
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
  const { t } = useI18n();
  const { data: summary } = usage;

  return (
    <section id={id} className="asb-provider-usage" aria-label={t("usage.panel.aria", { name })}>
      <header className="asb-provider-usage-head">
        <div className="asb-provider-usage-title">
          <h3 className="asb-section-title">{t("usage.panel.details")}</h3>
        </div>
        <div className="asb-provider-usage-actions">
          {onConfigure && (
            <Button
              variant="unstyled"
              className="asb-provider-usage-configure"
              onClick={onConfigure}
            >
              {t("usage.panel.editQuery")}
            </Button>
          )}
        </div>
      </header>

      {summary && <UsageReadingsTable readings={summary.readings} ariaLabel={t("usage.panel.readingsAria", { name })} />}
    </section>
  );
}
