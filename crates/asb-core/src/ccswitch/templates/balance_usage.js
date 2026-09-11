// The source `balance` usage template, vendored from the pinned reference
// baseline (src-tauri/src/services/balance.rs): the reference's native
// per-provider balance endpoints (DeepSeek, StepFun, SiliconFlow, OpenRouter,
// Novita), selected from the profile's own address at query time. The
// detected provider is captured in `request` and reused by `extract`, whose
// runtime input carries only the response body.
(() => {
  const number = (raw) => {
    if (typeof raw === "number" && Number.isFinite(raw)) return raw;
    if (typeof raw === "string" && raw.trim() !== "") {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  };
  const endpoints = {
    deepseek: "https://api.deepseek.com/user/balance",
    stepfun: "https://api.stepfun.com/v1/accounts",
    siliconflow_cn: "https://api.siliconflow.cn/v1/user/info",
    siliconflow_com: "https://api.siliconflow.com/v1/user/info",
    openrouter: "https://openrouter.ai/api/v1/credits",
    novita: "https://api.novita.ai/v3/user/balance",
  };
  const detect = (input) => {
    const base = String(input.baseUrl || "").toLowerCase();
    if (base.includes("api.deepseek.com")) return "deepseek";
    if (base.includes("api.stepfun.ai") || base.includes("api.stepfun.com")) return "stepfun";
    if (base.includes("api.siliconflow.cn")) return "siliconflow_cn";
    if (base.includes("api.siliconflow.com")) return "siliconflow_com";
    if (base.includes("openrouter.ai")) return "openrouter";
    if (base.includes("api.novita.ai")) return "novita";
    return null;
  };
  let provider = null;
  return {
    request(input) {
      provider = detect(input);
      if (provider === null) {
        throw new TypeError("balance 模板没有该服务地址对应的官方余额端点");
      }
      return {
        url: endpoints[provider],
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
      if (provider === "deepseek") {
        if (body.is_available === false) {
          throw new TypeError("余额不可用（insufficient balance）");
        }
        const infos = Array.isArray(body.balance_infos) ? body.balance_infos : [];
        if (infos.length === 0) {
          throw new TypeError("invalid imported usage result");
        }
        return infos.map((info) => ({
          planName: typeof info.currency === "string" ? info.currency : "CNY",
          remaining: number(info.total_balance),
          used: null,
          total: null,
          unit: typeof info.currency === "string" ? info.currency : "CNY",
        }));
      }
      if (provider === "stepfun") {
        return {
          planName: "StepFun",
          remaining: number(body.balance),
          used: null,
          total: null,
          unit: "CNY",
        };
      }
      if (provider === "siliconflow_cn" || provider === "siliconflow_com") {
        const cn = provider === "siliconflow_cn";
        return {
          planName: cn ? "SiliconFlow" : "SiliconFlow (EN)",
          remaining: number(body.data && body.data.totalBalance),
          used: null,
          total: null,
          unit: cn ? "CNY" : "USD",
        };
      }
      if (provider === "openrouter") {
        const data = body.data || body;
        const total = number(data.total_credits);
        const used = number(data.total_usage);
        const remaining = total !== null && used !== null ? total - used : null;
        if (remaining !== null && remaining <= 0) {
          throw new TypeError("没有剩余额度（no credits remaining）");
        }
        return { planName: "OpenRouter", remaining, used, total, unit: "USD" };
      }
      // Novita amounts are denominated in 0.0001 USD.
      const available = number(body.availableBalance);
      return {
        planName: "Novita AI",
        remaining: available === null ? null : available / 10000,
        used: null,
        total: null,
        unit: "USD",
      };
    },
  };
})()
