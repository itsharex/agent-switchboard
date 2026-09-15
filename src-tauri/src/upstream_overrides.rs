use asb_core::contracts::ProviderConnectionOptions;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

pub(crate) fn apply_body_override(
    body: &mut Vec<u8>,
    options: &ProviderConnectionOptions,
) -> Result<(), String> {
    let Some(patch) = options
        .local_proxy_request_overrides
        .as_ref()
        .map(|overrides| &overrides.body)
        .filter(|value| !value.is_null())
    else {
        return Ok(());
    };
    if !patch.is_object() {
        return Err("request body override must be a JSON object".to_string());
    }
    let mut target: Value = serde_json::from_slice(body)
        .map_err(|_| "converted upstream request is not valid JSON".to_string())?;
    if !target.is_object() {
        return Err("converted upstream request must be a JSON object".to_string());
    }
    merge_json(&mut target, patch, true);
    *body = serde_json::to_vec(&target)
        .map_err(|_| "request body override serialization failed".to_string())?;
    Ok(())
}

pub(crate) fn apply_header_overrides(headers: &mut HeaderMap, options: &ProviderConnectionOptions) {
    if let Some(overrides) = options.local_proxy_request_overrides.as_ref() {
        for (raw_name, raw_value) in &overrides.headers {
            let Ok(name) = HeaderName::from_bytes(raw_name.trim().as_bytes()) else {
                continue;
            };
            if is_protected(name.as_str()) {
                continue;
            }
            let Ok(value) = HeaderValue::from_str(raw_value) else {
                continue;
            };
            headers.insert(name, value);
        }
    }
    if let Some(user_agent) = options.custom_user_agent.as_deref() {
        if let Ok(value) = HeaderValue::from_str(user_agent) {
            headers.insert(USER_AGENT, value);
        }
    }
}

fn merge_json(target: &mut Value, patch: &Value, top_level: bool) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                if top_level && key == "stream" {
                    continue;
                }
                match target.get_mut(key) {
                    Some(existing) => merge_json(existing, value, false),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn is_protected(name: &str) -> bool {
    asb_core::contracts::PROTECTED_OVERRIDE_HEADERS
        .iter()
        .any(|protected| protected.eq_ignore_ascii_case(name))
}
