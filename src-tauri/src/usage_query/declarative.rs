use asb_core::contracts::{
    ProviderConnectionOptions, UpstreamProtocol, UsageReading, UsageSummary,
};

/// Substitutes placeholders in the stored URL. `{{baseUrl}}` loses any
/// trailing slash so `/user/balance` style paths concatenate cleanly.
pub(super) fn render_url(template: &str, api_key: &str, base_url: Option<&str>) -> String {
    template
        .replace("{{baseUrl}}", base_url.unwrap_or("").trim_end_matches('/'))
        .replace("{{apiKey}}", api_key)
}

/// Reads one number out of the response body via a JSON Pointer such as
/// `data/balance`; numeric strings count, everything else is absent.
pub(super) fn pointer_number(value: &serde_json::Value, path: &str) -> Option<f64> {
    let trimmed = path.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match value.pointer(&format!("/{trimmed}")) {
        Some(serde_json::Value::Number(number)) => number.as_f64(),
        Some(serde_json::Value::String(text)) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Checks the fixed URL form before it reaches the network. This deliberately
/// does not return the submitted URL, which could contain `{{apiKey}}`.
pub(super) fn is_http_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.trim() == url
        && !url.chars().any(char::is_control)
}

/// Picks the summary fields out of a parsed declarative response body. A path
/// that leads nowhere simply leaves the field unset; finding none of the
/// configured numbers at all is an error.
pub(super) fn extract_declarative_summary(
    body: &serde_json::Value,
    remaining_path: Option<&str>,
    used_path: Option<&str>,
    total_path: Option<&str>,
    unit: Option<String>,
    at: String,
) -> Result<UsageSummary, String> {
    let remaining = remaining_path.and_then(|path| pointer_number(body, path));
    let used = used_path.and_then(|path| pointer_number(body, path));
    let total = total_path.and_then(|path| pointer_number(body, path));
    if remaining.is_none() && used.is_none() && total.is_none() {
        return Err("响应中未找到任何配置的用量字段，请检查提取路径".to_string());
    }
    Ok(UsageSummary {
        readings: vec![UsageReading {
            plan_name: None,
            remaining,
            used,
            total,
            unit,
        }],
        at,
    })
}

pub(super) fn run_declarative_query(
    url_template: &str,
    remaining_path: Option<&str>,
    used_path: Option<&str>,
    total_path: Option<&str>,
    unit: Option<String>,
    api_key: &str,
    base_url: Option<&str>,
    upstream_protocol: UpstreamProtocol,
    authentication: Option<asb_core::AuthenticationScheme>,
    connection: &ProviderConnectionOptions,
) -> Result<UsageSummary, String> {
    let url = render_url(url_template, api_key, base_url);
    if !is_http_url(&url) {
        return Err("查询地址必须是 http(s) URL".to_string());
    }
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let headers = crate::probe::provider_auth_headers(api_key, upstream_protocol, authentication)?;
    let (status, body) = crate::probe::http_get_with_options(&url, &headers, connection)?;
    if status == 401 || status == 403 {
        return Err(format!(
            "服务地址拒绝了 API 密钥（HTTP {status}），请确认密钥仍然有效"
        ));
    }
    if !(200..300).contains(&status) {
        return Err(format!("用量查询返回 HTTP {status}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| "用量响应不是有效 JSON".to_string())?;
    extract_declarative_summary(&value, remaining_path, used_path, total_path, unit, at)
}
