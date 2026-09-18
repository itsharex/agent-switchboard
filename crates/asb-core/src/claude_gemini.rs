//! Claude's Google native endpoint and credential contract, not a Gemini client.
use crate::{AuthenticationScheme, UpstreamProtocol};
use serde_json::Value;
use url::Url;

pub fn request_endpoint(
    base: &str,
    full: bool,
    model: &str,
    stream: bool,
) -> Result<String, String> {
    let model = model.strip_prefix("models/").unwrap_or(model);
    if model.is_empty()
        || model.len() > 256
        || model.trim() != model
        || model
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '?' | '#' | '\\'))
        || matches!(model, "." | ".." | "{model}")
    {
        return Err("Gemini 模型 ID 无效".into());
    }
    let mut url = root(base, full)?;
    url.path_segments_mut()
        .map_err(|_| "Gemini 服务地址无效")?
        .pop_if_empty()
        .push("models")
        .push(&format!(
            "{model}:{}",
            if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            }
        ));
    let query = url
        .query_pairs()
        .filter(|(key, _)| key != "alt")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !query.is_empty() || stream {
        let mut pairs = url.query_pairs_mut();
        pairs.extend_pairs(query);
        if stream {
            pairs.append_pair("alt", "sse");
        }
    }
    Ok(url.to_string())
}

/// Display only. Execution must call request_endpoint with the selected model.
pub fn request_preview(base: &str, full: bool, model: Option<&str>) -> Result<String, String> {
    match model.filter(|model| !model.is_empty()) {
        Some(model) => request_endpoint(base, full, model, false),
        None => request_endpoint(base, full, "ASB_MODEL_PLACEHOLDER", false)
            .map(|url| url.replace("/ASB_MODEL_PLACEHOLDER:", "/{model}:")),
    }
}

pub fn models_endpoint(base: &str, full: bool) -> Result<String, String> {
    let mut url = root(base, full)?;
    url.path_segments_mut()
        .map_err(|_| "Gemini 服务地址无效")?
        .pop_if_empty()
        .push("models");
    let query = url
        .query_pairs()
        .filter(|(key, _)| key != "alt")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url.to_string())
}

fn root(base: &str, full: bool) -> Result<Url, String> {
    if full {
        crate::endpoint::validate_full_url(base)?;
    } else {
        crate::endpoint::validate_base_url(base, UpstreamProtocol::GeminiGenerateContent)?;
    }
    let mut url = Url::parse(base).map_err(|_| "Gemini 服务地址无效")?;
    let path = url.path().trim_end_matches('/');
    let prefix = if full {
        let (prefix, operation) = path.rsplit_once("/models/").ok_or(
            "Gemini 完整地址必须以 /models/<model>:generateContent 或 :streamGenerateContent 结尾",
        )?;
        if !operation.ends_with(":generateContent")
            && !operation.ends_with(":streamGenerateContent")
        {
            return Err("Gemini 完整地址不是 generateContent 端点".into());
        }
        prefix.to_string()
    } else if path.ends_with(":streamGenerateContent") {
        return Err("Gemini 服务根地址不能包含完整请求端点".into());
    } else if path.ends_with("/v1") || path.ends_with("/v1beta") || path.ends_with("/v1alpha") {
        path.to_string()
    } else {
        format!("{path}/v1beta")
    };
    url.set_path(&prefix);
    Ok(url)
}

/// Structured Google OAuth remains intact in the profile; only its access token
/// is delivered to the upstream. No native CLI credential cache is read.
pub fn credential(
    raw: &str,
    selected: Option<AuthenticationScheme>,
) -> Result<(AuthenticationScheme, String), String> {
    let structured = raw.trim_start().starts_with('{');
    let (token, scheme) = if structured {
        let value: Value =
            serde_json::from_str(raw).map_err(|_| "Google OAuth 凭据不是有效 JSON")?;
        let token = value
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.trim().is_empty())
            .ok_or("Google OAuth 凭据缺少 access_token，请更新授权凭据")?;
        if let Some(expiry) = value.get("expiry_date").filter(|value| !value.is_null()) {
            let expiry = expiry
                .as_u64()
                .ok_or("Google OAuth expiry_date 必须是毫秒时间戳")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "本机时间早于 Unix 纪元")?
                .as_millis();
            if u128::from(expiry) <= now {
                return Err("Google OAuth access_token 已过期，请更新授权凭据".into());
            }
        }
        (token.to_string(), AuthenticationScheme::Bearer)
    } else if raw.starts_with("ya29.") {
        (raw.to_string(), AuthenticationScheme::Bearer)
    } else {
        (
            raw.to_string(),
            selected.unwrap_or(AuthenticationScheme::XGoogApiKey),
        )
    };
    if token.trim().is_empty() || token.chars().any(char::is_control) {
        return Err("Google 凭据为空或包含无效请求头字符".into());
    }
    if scheme == AuthenticationScheme::XApiKey
        || (structured && selected.is_some_and(|v| v != AuthenticationScheme::Bearer))
    {
        return Err("Gemini Native 只支持 x-goog-api-key 或 Bearer 认证".into());
    }
    Ok((scheme, token))
}

