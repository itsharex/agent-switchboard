// The Zhipu GLM coding-plan quota endpoint, vendored from the pinned
// reference baseline (src-tauri/src/services/coding_plan.rs, query_zhipu):
// raw-key authorization, host derived from the profile's own address, and
// token/credit limit entries classified by their `unit` window. Personal and
// team plans share one response shape.
(() => {
  const number = (raw) => {
    if (typeof raw === "number" && Number.isFinite(raw)) return raw;
    if (typeof raw === "string" && raw.trim() !== "") {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  };
  return {
    request(input) {
      const base = String(input.baseUrl || "").toLowerCase();
      const origin = base.includes("bigmodel.cn")
        ? "https://open.bigmodel.cn"
        : "https://api.z.ai";
      return {
        url: origin + "/api/monitor/usage/quota/limit",
        method: "GET",
        headers: {
          Authorization: String(input.apiKey || ""),
          "Content-Type": "application/json",
          "Accept-Language": "en-US,en",
        },
      };
    },
    extract(input) {
      if (input.status < 200 || input.status >= 300) {
        throw new TypeError("imported usage request was not successful");
      }
      const body = input.body;
      if (body && body.success === false) {
        throw new TypeError(
          "用量接口错误：" + (typeof body.msg === "string" ? body.msg : "unknown"),
        );
      }
      const limits =
        body && body.data && Array.isArray(body.data.limits) ? body.data.limits : [];
      const tiers = [];
      for (const item of limits) {
        const type = String(item.type || "").toUpperCase();
        if (type !== "TOKENS_LIMIT" && type !== "CREDIT_LIMIT") continue;
        // The upstream reports utilization as a percentage; projected onto
        // the native reading fields as parts of a hundred.
        const percentage = number(item.percentage) || 0;
        const tier = {
          used: percentage,
          total: 100,
          remaining: Math.max(0, 100 - percentage),
          unit: null,
        };
        if (item.unit === 3) tier.planName = "5 小时窗口";
        else if (item.unit === 6) tier.planName = "每周窗口";
        tiers.push(tier);
      }
      if (tiers.length === 0) {
        throw new TypeError("invalid imported usage result");
      }
      return tiers;
    },
  };
})()
