import { useI18n } from "../i18n";
import type { ProviderUsage } from "./use-provider-usage";
import { UsageSummaryView } from "./UsageSummaryView";

export function ProviderUsageSummary({ name, usage }: { name: string; usage: ProviderUsage }) {
  const { t } = useI18n();
  return <UsageSummaryView name={name} readings={usage.data?.readings ?? []} at={usage.data?.at ?? null}
    querying={usage.querying} error={usage.error} run={usage.run} empty={t("usage.summary.notQueried")} />;
}
