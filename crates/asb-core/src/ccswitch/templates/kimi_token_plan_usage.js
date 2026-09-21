// The Kimi For Coding quota endpoint, vendored from the pinned reference
// baseline (src-tauri/src/services/coding_plan.rs, query_kimi): a fixed
// endpoint with the profile key; absolute limit/remaining numbers per window.
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
        url: "https://api.kimi.com/coding/v1/usages",
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
      const tiers = [];
      const window = (detail, name) => {
        const total = number(detail.limit);
        const remaining = number(detail.remaining);
        const tier = {
          remaining,
          used: total !== null && remaining !== null ? total - remaining : null,
          total,
          unit: null,
          resetsAt: resetTime(detail.resetTime),
        };
        tier.planName = name;
        return tier;
      };
      const limits = Array.isArray(body.limits) ? body.limits : [];
      for (const item of limits) {
        if (item && item.detail) {
          tiers.push(window(item.detail, "5 小时窗口"));
        }
      }
      if (body.usage) {
        tiers.push(window(body.usage, "每周窗口"));
      }
      if (tiers.length === 0) {
        throw new TypeError("invalid imported usage result");
      }
      return tiers;
    },
  };
})()
