use super::*;
use serde_json::Value;
pub fn from_config(config: &Value) -> Result<Option<(ClaudeNative, Option<String>)>, String> {
    let Some(env) = config.get("env") else {
        return Ok(None);
    };
    let env = env.as_object().ok_or("Claude env 必须是对象")?;
    let enabled = KINDS
        .into_iter()
        .filter(|kind| enabled(env.get(kind.flag())))
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return Ok(None);
    }
    if enabled.len() != 1 {
        return Err("Claude 原生云模式不能同时启用 Bedrock、Vertex 或 Foundry".into());
    }
    let kind = enabled[0];
    for key in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        if env
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
        {
            return Err(format!(
                "{} 使用原生 SDK 凭据，不读取 {key}；请使用对应的云认证字段",
                kind.flag()
            ));
        }
    }
    let mut environment = BTreeMap::new();
    for (key, value) in env.iter().filter(|(key, _)| kind.accepts(key)) {
        if value.is_null() {
            continue;
        }
        let value = match value {
            Value::String(v) => v.clone(),
            Value::Bool(v) => if *v { "1" } else { "0" }.into(),
            Value::Number(v) => v.to_string(),
            _ => return Err(format!("Claude 原生云环境键 {key} 必须是字符串")),
        };
        if !value.is_empty() {
            environment.insert(key.clone(), value);
        }
    }
    let base = env
        .get(kind.base_key())
        .or_else(|| env.get("ANTHROPIC_BASE_URL"))
        .filter(|v| !v.is_null())
        .map(|v| v.as_str().ok_or("Claude 原生云服务根地址必须是字符串"))
        .transpose()?
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string);
    if kind == ClaudeNativeKind::Bedrock && !environment.contains_key("AWS_BEARER_TOKEN_BEDROCK") {
        if let Some(key) = config
            .get("apiKey")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        {
            environment.insert("AWS_BEARER_TOKEN_BEDROCK".into(), key.into());
        }
    }
    let native = ClaudeNative { kind, environment };
    native.validate(base.as_deref())?;
    Ok(Some((native, base)))
}
fn enabled(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(value)) => {
            !["", "0", "false"].contains(&value.trim().to_ascii_lowercase().as_str())
        }
        Some(Value::Number(value)) => value.as_u64() != Some(0),
        _ => true,
    }
}
