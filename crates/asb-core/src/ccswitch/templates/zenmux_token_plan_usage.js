// The ZenMux plan-usage endpoint, vendored from the pinned reference baseline
// (src-tauri/src/services/coding_plan.rs, query_zenmux): the profile's own
// address is the endpoint; dollar values win over the percentage fallback.
(() => {
  const number = (raw) => {
    if (typeof raw === "number" && Number.isFinite(raw)) return raw;
    if (typeof raw === "string" && raw.trim() !== "") {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  };
  const resetTime = (raw) => {
    if (raw === null || raw === undefined || raw === "") return undefined;
    const value = typeof raw === "number" ? (raw < 1e12 ? raw * 1000 : raw) : raw;
    const date = new Date(value);
    return Number.isFinite(date.getTime()) && date.getTime() > 0 ? date.toISOString() : undefined;
  };
  return {
    request(input) {
      return {
        url: String(input.baseUrl || "").replace(/\/+$/, ""),
        method: "GET",
        headers: {
          Authorization: "Bearer " + String(input.apiKey || ""),
          Accept: "application/json",
        },
      };
    },
    extract(input) {
      if (input.status < 200 || input.status >= 300) {
        throw new TypeError("imported usage request was not successful");
      }
      const body = input.body;
      if (body && body.success !== true) {
        throw new TypeError(
          "用量接口错误：" +
            (typeof body.message === "string" ? body.message : "unknown"),
        );
      }
      const data = (body && body.data) || {};
      const tiers = [];
      const window = (entry, name) => {
        if (!entry) return;
        const percentage = number(entry.usage_percentage);
        const used = number(entry.used_value_usd);
        const total = number(entry.max_value_usd);
        if (total !== null) {
          tiers.push({
            planName: name,
            remaining: used !== null ? total - used : null,
            used,
            total,
            unit: "USD",
            resetsAt: resetTime(entry.resets_at),
          });
          return;
        }
        if (percentage !== null) {
          const usedPercent = percentage * 100;
          tiers.push({
            planName: name,
            remaining: Math.max(0, 100 - usedPercent),
            used: usedPercent,
            total: 100,
            unit: "%",
            resetsAt: resetTime(entry.resets_at),
          });
        }
      };
      window(data.quota_5_hour, "5 小时窗口");
      window(data.quota_7_day, "每周窗口");
      if (tiers.length === 0) {
        throw new TypeError("invalid imported usage result");
      }
      return tiers;
    },
  };
})()
