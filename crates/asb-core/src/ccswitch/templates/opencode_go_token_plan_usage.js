// The OpenCode Go usage endpoint, vendored from the pinned reference
// baseline (src-tauri/src/services/coding_plan.rs, query_opencode_go): a
// first-party undocumented route, Bearer-only, reporting used percentages
// for up to three windows; unparsable windows are skipped, not fatal.
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
      return {
        url: "https://opencode.ai/zen/go/v1/usage",
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
      const usage = input.body && input.body.usage;
      if (!usage) {
        throw new TypeError("invalid imported usage result");
      }
      const windows = [
        ["rolling", "5 小时窗口"],
        ["weekly", "每周窗口"],
        ["monthly", "每月窗口"],
      ];
      const tiers = [];
      for (const entry of windows) {
        const key = entry[0];
        const name = entry[1];
        const percent = usage[key] ? number(usage[key].percent) : null;
        if (percent === null) continue;
        tiers.push({
          planName: name,
          remaining: Math.max(0, 100 - percent),
          used: percent,
          total: 100,
          unit: null,
        });
      }
      if (tiers.length === 0) {
        throw new TypeError("invalid imported usage result");
      }
      return tiers;
    },
  };
})()
