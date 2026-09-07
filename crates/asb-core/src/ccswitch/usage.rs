use std::collections::BTreeSet;

use serde_json::Value;

use crate::contracts::UsageQuery;

/// Usage-script keys which represent an independent endpoint or credential.
/// The profile's own `baseUrl` and `apiKey` are the only supported query
/// inputs, so importing one of these would silently change the query's owner.
const USAGE_SCRIPT_INPUT_OVERRIDES: [&str; 7] = [
    "apiKey",
    "baseUrl",
    "accessToken",
    "userId",
    "accessKeyId",
    "secretAccessKey",
    "teamId",
];

const USAGE_SCRIPT_REPRESENTED_KEYS: [&str; 5] =
    ["enabled", "language", "code", "templateType", "timeout"];

/// Converts one enabled source JavaScript program into the application's
/// native script contract. This is an import-time compilation step: durable
/// profiles never retain a source-specific query kind or a runtime compatibility
/// branch.
fn compile_usage_script(source: &str) -> String {
    [
        r#"(() => {
  const cc = ("#,
        source,
        r#");
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
    if (!object(value) || value.isValid === false) {
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
})()"#,
    ]
    .concat()
}

fn contains_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        _ => true,
    }
}

fn has_unsupported_placeholder(source: &str) -> bool {
    let mut remaining = source;
    while let Some(start) = remaining.find("{{") {
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        let name = after_start[..end].trim();
        if name != "apiKey" && name != "baseUrl" {
            return true;
        }
        remaining = &after_start[end + 2..];
    }
    false
}

/// Resolves the only query-script source supported by this import.
/// Warnings name source fields only; values and script text never appear in
/// diagnostics or scan data.
pub(super) fn map_usage_query(
    meta: Option<&str>,
    warnings: &mut Vec<String>,
) -> Option<UsageQuery> {
    let Some(text) = meta.filter(|text| !text.trim().is_empty()) else {
        return None;
    };
    let meta: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            warnings.push("未导入: meta.usage_script（元数据无法解析）".to_string());
            return None;
        }
    };
    let Some(script) = meta.get("usage_script") else {
        return None;
    };
    let Some(script) = script.as_object() else {
        warnings.push("未导入: meta.usage_script（格式无效）".to_string());
        return None;
    };
    if script.get("enabled").and_then(Value::as_bool) != Some(true) {
        warnings.push("未导入: meta.usage_script（脚本已禁用）".to_string());
        return None;
    }
    if !script
        .get("language")
        .and_then(Value::as_str)
        .is_some_and(|language| language.eq_ignore_ascii_case("javascript"))
    {
        warnings.push("未导入: meta.usage_script（仅支持 JavaScript 脚本）".to_string());
        return None;
    }
    let source = match script
        .get("code")
        .and_then(Value::as_str)
        .filter(|source| !source.trim().is_empty())
    {
        Some(source) => source,
        None if script.get("templateType").and_then(Value::as_str).is_some() => {
            warnings.push(
                "未导入: meta.usage_script.templateType（内建模板没有可导入的源码）".to_string(),
            );
            return None;
        }
        None => {
            warnings.push("未导入: meta.usage_script.code（没有可导入的源码）".to_string());
            return None;
        }
    };
    for key in USAGE_SCRIPT_INPUT_OVERRIDES {
        if script.get(key).is_some_and(contains_value) {
            warnings.push(format!("未导入: meta.usage_script.{key}"));
            return None;
        }
    }
    if has_unsupported_placeholder(source) {
        warnings.push("未导入: meta.usage_script.code（包含无对应输入的占位符）".to_string());
        return None;
    }

    let mut unsupported = BTreeSet::new();
    for key in script.keys() {
        if !USAGE_SCRIPT_REPRESENTED_KEYS.contains(&key.as_str())
            && !USAGE_SCRIPT_INPUT_OVERRIDES.contains(&key.as_str())
        {
            unsupported.insert(key);
        }
    }
    for key in unsupported {
        warnings.push(format!("未导入: meta.usage_script.{key}"));
    }
    if script.get("timeout").is_some_and(contains_value) {
        warnings.push("未导入: meta.usage_script.timeout".to_string());
    }
    Some(UsageQuery::Script {
        source: compile_usage_script(source),
        // The source interval lands in the unsupported-key warnings; imported
        // queries start manual-only.
        refresh_interval_minutes: 0,
    })
}
