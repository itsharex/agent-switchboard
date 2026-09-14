//! Source-only authentication aliases are normalized here, never kept as live internal fallbacks.

use crate::contracts::{
    LocalProxyRequestOverrides, ProviderAuthBinding, ProviderConnectionOptions,
};
use serde_json::{json, Value};

pub(super) fn import(
    meta: &Value,
    endpoint: Option<&str>,
    connection: &mut ProviderConnectionOptions,
) -> Result<(), String> {
    if connection.provider_type.is_none() {
        let endpoint = endpoint.unwrap_or("");
        if endpoint.starts_with("https://api.githubcopilot.com")
            || endpoint.starts_with("https://api.individual.githubcopilot.com")
        {
            connection.provider_type = Some("github_copilot".into());
        } else if endpoint.starts_with("https://chatgpt.com/backend-api/codex") {
            connection.provider_type = Some("codex_oauth".into());
        }
    }
    if let Some(raw) = meta.get("authBinding").or_else(|| meta.get("auth_binding")) {
        let mut raw = raw.clone();
        let object = raw.as_object_mut().ok_or("meta.authBinding 必须是对象")?;
        for (old, current) in [
            ("auth_provider", "authProvider"),
            ("account_id", "accountId"),
        ] {
            if let Some(value) = object.remove(old) {
                if object.insert(current.into(), value).is_some() {
                    return Err(format!("meta.authBinding.{current} 重复"));
                }
            }
        }
        connection.auth_binding = Some(
            serde_json::from_value::<ProviderAuthBinding>(raw)
                .map_err(|_| "meta.authBinding 认证绑定格式无效")?,
        );
    } else if let Some(id) = meta.get("githubAccountId") {
        let id = id
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("meta.githubAccountId 必须是账号 ID 字符串")?;
        connection.auth_binding = Some(ProviderAuthBinding {
            source: "managed_account".into(),
            auth_provider: Some("github_copilot".into()),
            account_id: Some(id.into()),
        });
    }
    crate::claude_auth::managed_auth(connection)?;
    if let Some(fast) = meta.get("codexFastMode") {
        let fast = fast.as_bool().ok_or("meta.codexFastMode 必须是布尔值")?;
        if fast {
            let overrides = connection
                .local_proxy_request_overrides
                .get_or_insert_with(|| LocalProxyRequestOverrides {
                    headers: Default::default(),
                    body: json!({}),
                });
            if overrides.body.is_null() {
                overrides.body = json!({});
            }
            let object = overrides
                .body
                .as_object_mut()
                .ok_or("Claude 请求覆盖必须是对象")?;
            object.insert("service_tier".into(), json!("priority"));
        }
    }
    Ok(())
}
