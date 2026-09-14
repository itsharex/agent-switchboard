use super::*;
use crate::{
    ownership::{setting_specs, SettingOwner},
    AppKind,
};
pub fn validate(extra: &Extra) -> Result<(), String> {
    let value = Value::Object(extra.clone());
    if value.to_string().len() > 256 * 1024 {
        return Err("Claude 额外通用配置超过 256 KiB".into());
    }
    for (pointer, value) in leaves(&value)? {
        let segments = super::pointer::decode(&pointer)?;
        path(&segments)?;
        if segments.first().is_some_and(|key| key == "env")
            && (segments.len() != 2 || !value.is_string())
        {
            return Err("Claude 通用环境变量必须是 env 下的字符串值".into());
        }
        values(&value, 0)?;
    }
    Ok(())
}
pub(super) fn path(path: &[String]) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 32
        || path
            .iter()
            .any(|key| key.is_empty() || key.len() > 256 || key.chars().any(char::is_control))
    {
        return Err("Claude 通用配置字段名无效".into());
    }
    let first = path[0].as_str();
    if matches!(
        first,
        "mcpServers"
            | "enabledMcpjsonServers"
            | "disabledMcpjsonServers"
            | "enableAllProjectMcpServers"
            | "enabledPlugins"
            | "extraKnownMarketplaces"
    ) {
        return Err(format!(
            "{first} 由 Claude MCP/扩展模块管理，不能重复放入通用配置"
        ));
    }
    if first == "env"
        && path
            .get(1)
            .is_some_and(|key| key.starts_with("ASB_") || crate::claude_native::is_native_env(key))
    {
        return Err("Claude 原生云凭据与本机路由标记不属于通用配置".into());
    }
    let key = path.join(".");
    for spec in setting_specs(AppKind::Claude) {
        if spec.key == key
            || spec.key.starts_with(&format!("{key}."))
            || key.starts_with(&format!("{}.", spec.key))
        {
            return Err(if spec.owner == SettingOwner::Provider {
                format!("配置片段不能包含供应商设置 {}", spec.key)
            } else {
                format!(
                    "{} 已由可视化客户端偏好管理，不能重复放入额外配置",
                    spec.key
                )
            });
        }
    }
    Ok(())
}
fn values(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("Claude 通用配置值嵌套超过限制".into());
    }
    match value {
        Value::Number(number) => {
            let limit = crate::contracts::MAX_EXACT_CONFIG_INTEGER;
            if number
                .as_i64()
                .is_some_and(|v| !(-limit..=limit).contains(&v))
                || number.as_u64().is_some_and(|v| v > limit as u64)
            {
                return Err("Claude 通用配置整数超出界面可无损保存的范围，请改用字符串".into());
            }
        }
        Value::Array(items) => {
            for value in items {
                values(value, depth + 1)?;
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                values(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
