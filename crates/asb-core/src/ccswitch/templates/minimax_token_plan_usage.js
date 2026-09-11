// The MiniMax coding-plan remains endpoint, vendored from the pinned
// reference baseline (src-tauri/src/services/coding_plan.rs, query_minimax):
// domain chosen from the profile's own address; the "general" model entry
// reports remaining percentages per window (weekly only while activated).
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
      const domain = String(input.baseUrl || "").toLowerCase().includes("api.minimax.io")
        ? "api.minimax.io"
        : "api.minimaxi.com";
      return {
        url: "https://" + domain + "/v1/api/openplatform/coding_plan/remains",
        method: "GET",
        headers: {
          Authorization: "Bearer " + String(input.apiKey || ""),
          "Content-Type": "application/json",
        },
      };
    },
    extract(input) {
      if (input.status < 200 || input.status >= 300) {
        throw new TypeError("imported usage request was not successful");
      }
      const body = input.body;
      if (body && body.base_resp && body.base_resp.status_code !== 0) {
        throw new TypeError(
          "用量接口错误：" +
            (typeof body.base_resp.status_msg === "string"
              ? body.base_resp.status_msg
              : "unknown"),
        );
      }
      const remains = Array.isArray(body.model_remains) ? body.model_remains : [];
      const item = remains.find(
        (entry) => entry && entry.model_name === "general",
      );
      if (!item) {
        throw new TypeError("invalid imported usage result");
      }
      const tiers = [];
      const window = (remainPercent, name) => ({
        planName: name,
        remaining: remainPercent,
        used: 100 - remainPercent,
        total: 100,
        unit: null,
      });
      const interval = number(item.current_interval_remaining_percent);
      if (interval !== null) {
        tiers.push(window(interval, "5 小时窗口"));
      }
      if (item.current_weekly_status === 1) {
        const weekly = number(item.current_weekly_remaining_percent);
        if (weekly !== null) {
          tiers.push(window(weekly, "每周窗口"));
        }
      }
      if (tiers.length === 0) {
        throw new TypeError("invalid imported usage result");
      }
      return tiers;
    },
  };
})()
