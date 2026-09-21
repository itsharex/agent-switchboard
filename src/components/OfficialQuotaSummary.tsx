import { officialQuotaReadings } from "../lib/usage-format";
import type { CodexOfficialQuota } from "../api/client";
import type { CachedQuery } from "./use-cached-query";
import { UsageSummaryView } from "./UsageSummaryView";

export function OfficialQuotaSummary({ name, quota }: { name: string; quota: CachedQuery<CodexOfficialQuota> }) {
  const status = quota.data?.status;
  const empty = status === "signInRequired" ? "请登录后读取额度"
    : status === "reauthenticationRequired" ? "请重新登录后读取额度"
    : status === "unavailable" ? "额度读取失败" : "尚未查询额度";
  const error = quota.error ?? (status === "unavailable" ? "官方额度读取失败，请重试。"
    : status === "reauthenticationRequired" ? "登录已过期，请重新登录。"
    : status === "signInRequired" ? "请登录后读取额度。" : null);
  return <UsageSummaryView name={name} readings={quota.data ? officialQuotaReadings(quota.data) : []}
    at={quota.data?.at ?? null} querying={quota.querying} error={error} stale={quota.data?.stale}
    empty={empty} run={quota.run} />;
}
