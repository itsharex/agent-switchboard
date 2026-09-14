//! Claude model spellings accepted by Copilot. Explicit model selections are never replaced by a pricier model.

use serde_json::Value;

pub(crate) fn apply(bytes: &mut Vec<u8>, original: &[u8]) -> Result<(), String> {
    let source: Value = serde_json::from_slice(original).map_err(|_| "Claude 原始请求无法解析")?;
    let mut body: Value =
        serde_json::from_slice(bytes).map_err(|_| "Claude Copilot 请求无法解析")?;
    if let Some(model) = body.get("model").and_then(Value::as_str) {
        let one_m = source
            .get("model")
            .and_then(Value::as_str)
            .is_some_and(one_m_suffix);
        body["model"] = Value::String(normalize(model, one_m));
    }
    *bytes = serde_json::to_vec(&body).map_err(|_| "Claude Copilot 请求无法编码")?;
    Ok(())
}

fn one_m_suffix(value: &str) -> bool {
    value
        .as_bytes()
        .get(value.len().saturating_sub(4)..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(b"[1m]"))
}

fn normalize(model: &str, requested_one_m: bool) -> String {
    if !model
        .as_bytes()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"claude-"))
    {
        return model.into();
    }
    let one_m = requested_one_m || one_m_suffix(model) || model.ends_with("-1m");
    let mut base = if one_m_suffix(model) {
        &model[..model.len() - 4]
    } else {
        model.strip_suffix("-1m").unwrap_or(model)
    };
    if let Some((head, date)) = base.rsplit_once('-') {
        if date.len() == 8 && date.bytes().all(|byte| byte.is_ascii_digit()) {
            base = head;
        }
    }
    let mut normalized = base.to_string();
    if let Some((head, minor)) = base.rsplit_once('-') {
        if let Some((_, major)) = head.rsplit_once('-') {
            if major.parse::<u32>().is_ok_and(|n| n >= 4)
                && !minor.is_empty()
                && minor.bytes().all(|byte| byte.is_ascii_digit())
            {
                normalized = format!("{head}.{minor}");
            }
        }
    }
    if one_m {
        normalized.push_str("-1m");
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_only_supported_claude_version_spellings_and_preserves_one_m() {
        assert_eq!(
            normalize("claude-sonnet-4-6-20260101", true),
            "claude-sonnet-4.6-1m"
        );
        assert_eq!(
            normalize("claude-opus-4.7[1M]", false),
            "claude-opus-4.7-1m"
        );
        assert_eq!(normalize("claude-3-5-sonnet", false), "claude-3-5-sonnet");
        assert_eq!(normalize("供应商模型🌟", true), "供应商模型🌟");
    }
}
