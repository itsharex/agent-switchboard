import { formatOfficialQuotaBalance } from "../lib/usage-format";
import type { CodexOfficialQuota } from "../api/client";
import type { CachedQuery } from "./use-cached-query";

export function OfficialQuotaSummary({ name, quota }: { name: string; quota: CachedQuery<CodexOfficialQuota> }) {
  const balance = quota.data ? formatOfficialQuotaBalance(quota.data, "row") : null;
  const status = quota.data?.status;
  const empty = status === "signInRequired" ? "请登录后读取额度"
    : status === "reauthenticationRequired" ? "请重新登录后读取额度"
    : quota.error || status === "unavailable" ? "额度读取失败"
    : quota.querying ? "正在读取额度" : "暂无额度读数";
  return (
    <span className="asb-row-usage-summary" aria-label={`${name} 官方用量余额`} title={quota.error ?? undefined}>
      {balance ?? empty}
      {balance && quota.error && " · 更新失败"}
      {balance && quota.querying && " · 更新中"}
    </span>
  );
}
