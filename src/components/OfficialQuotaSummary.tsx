import { officialQuotaReadings } from "../lib/usage-format";
import type { CodexOfficialQuota } from "../api/client";
import { useI18n } from "../i18n";
import type { CachedQuery } from "./use-cached-query";
import { UsageSummaryView } from "./UsageSummaryView";

export function OfficialQuotaSummary({ name, quota }: { name: string; quota: CachedQuery<CodexOfficialQuota> }) {
  const { t } = useI18n();
  const status = quota.data?.status;
  const empty = status === "signInRequired" ? t("clientConfig.quota.emptySignIn")
    : status === "reauthenticationRequired" ? t("clientConfig.quota.emptyReauth")
    : status === "unavailable" ? t("clientConfig.quota.emptyUnavailable") : t("clientConfig.quota.emptyDefault");
  const error = quota.error ?? (status === "unavailable" ? t("clientConfig.quota.errorUnavailable")
    : status === "reauthenticationRequired" ? t("clientConfig.quota.errorReauth")
    : status === "signInRequired" ? t("clientConfig.quota.errorSignIn") : null);
  return <UsageSummaryView name={name} readings={quota.data ? officialQuotaReadings(quota.data) : []}
    at={quota.data?.at ?? null} querying={quota.querying} error={error} stale={quota.data?.stale}
    empty={empty} run={quota.run} />;
}
