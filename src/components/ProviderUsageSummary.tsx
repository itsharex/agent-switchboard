import type { ProviderUsage } from "./use-provider-usage";
import { UsageSummaryView } from "./UsageSummaryView";

export function ProviderUsageSummary({ name, usage }: { name: string; usage: ProviderUsage }) {
  return <UsageSummaryView name={name} readings={usage.data?.readings ?? []} at={usage.data?.at ?? null}
    querying={usage.querying} error={usage.error} run={usage.run} empty="尚未查询用量" />;
}
