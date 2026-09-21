(() => {
  const cc = (__ASB_CC_SOURCE__);
  const object = (value) =>
    value !== null && typeof value === "object" && !Array.isArray(value);
  const substitute = (value, input) =>
    String(value)
      .replaceAll("{{baseUrl}}", String(input.baseUrl || "").replace(/\/+$/, ""))
      .replaceAll("{{apiKey}}", String(input.apiKey || ""));
  const number = (value) => {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim() !== "") {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  };
  const reading = (value) => {
    if (!object(value)) {
      throw new TypeError("invalid imported usage result");
    }
    const result = {
      remaining: number(value.remaining),
      used: number(value.used),
      total: number(value.total),
      unit: typeof value.unit === "string" ? value.unit : null,
    };
    if (typeof value.planName === "string" && value.planName.trim() !== "") {
      result.planName = value.planName;
    }
    if (typeof value.isValid === "boolean") result.isValid = value.isValid;
    if (value.isValid === false && typeof value.invalidMessage === "string") {
      result.invalidMessage = value.invalidMessage;
    }
    if (__ASB_TOKEN_PLAN__) {
      const used = number(value.used);
      result.unit = "%";
      result.total = used === null ? null : 100;
      result.remaining = used === null ? null : 100 - used;
      let metadata = null;
      if (typeof value.extra === "string" && value.extra.trim().startsWith("{")) {
        metadata = JSON.parse(value.extra);
        if (!object(metadata)) throw new TypeError("invalid token plan metadata");
      }
      const reset = metadata ? metadata.resetsAt : value.extra;
      if (typeof reset === "string" && reset.trim()) result.resetsAt = reset;
      if (metadata) {
        const details = [];
        if (typeof metadata.planLabel === "string") details.push(metadata.planLabel);
        const usedUsd = number(metadata.usedValueUsd);
        const totalUsd = number(metadata.maxValueUsd);
        if (usedUsd !== null) details.push("已用 " + usedUsd + " USD");
        if (totalUsd !== null) details.push("总量 " + totalUsd + " USD");
        if (details.length) result.extra = details.join(" · ");
      }
    } else if (typeof value.extra === "string" && value.extra.trim()) {
      result.extra = value.extra;
    }
    return result;
  };
  return {
    request(input) {
      if (!object(cc) || !object(cc.request) || typeof cc.extractor !== "function") {
        throw new TypeError("invalid imported usage script");
      }
      const sourceRequest = cc.request;
      const headers = {};
      if (sourceRequest.headers !== undefined) {
        if (!object(sourceRequest.headers)) {
          throw new TypeError("invalid imported usage request");
        }
        for (const name in sourceRequest.headers) {
          if (Object.prototype.hasOwnProperty.call(sourceRequest.headers, name)) {
            headers[name] = substitute(sourceRequest.headers[name], input);
          }
        }
      }
      const request = {
        url: substitute(sourceRequest.url, input),
        method: String(sourceRequest.method || "GET").toUpperCase(),
        headers,
      };
      if (sourceRequest.body !== undefined && sourceRequest.body !== null) {
        const body =
          typeof sourceRequest.body === "string"
            ? sourceRequest.body
            : JSON.stringify(sourceRequest.body);
        request.body = substitute(body, input);
      }
      return request;
    },
    extract(input) {
      if (input.status < 200 || input.status >= 300) {
        throw new TypeError("imported usage request was not successful");
      }
      const extracted = cc.extractor(input.body);
      return Array.isArray(extracted) ? extracted.map(reading) : reading(extracted);
    },
  };
})()
