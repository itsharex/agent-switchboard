// The source `general` usage template, vendored from the pinned reference
// baseline (src/components/UsageScriptModal.tsx, DEFAULT_SCRIPTS[GENERAL]):
// one GET on the profile's own /user/balance with the profile key; the
// balance is read as remaining USD.
(() => {
  const number = (raw) => {
    if (typeof raw === "number" && Number.isFinite(raw)) return raw;
    if (typeof raw === "string" && raw.trim() !== "") {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  };
  const substitute = (value, input) =>
    String(value)
      .replaceAll("{{baseUrl}}", String(input.baseUrl || "").replace(/\/+$/, ""))
      .replaceAll("{{apiKey}}", String(input.apiKey || ""));
  return {
    request(input) {
      return {
        url: substitute("{{baseUrl}}/user/balance", input),
        method: "GET",
        headers: {
          Authorization: substitute("Bearer {{apiKey}}", input),
          "User-Agent": "agent-switchboard/1.0",
        },
      };
    },
    extract(input) {
      if (input.status < 200 || input.status >= 300) {
        throw new TypeError("imported usage request was not successful");
      }
      return {
        remaining: number(input.body.balance),
        used: null,
        total: null,
        unit: "USD",
      };
    },
  };
})()
