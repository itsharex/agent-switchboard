//! Codex connection validation must not inherit Claude's authentication policy.
use super::ValidationError;
use crate::contracts::{AuthenticationScheme, ProviderConnectionOptions, UpstreamProtocol};
pub(crate) fn validate_connection(
    connection: &ProviderConnectionOptions,
    protocol: UpstreamProtocol,
    authentication: Option<AuthenticationScheme>,
) -> Result<bool, ValidationError> {
    let error = ValidationError::CodexProviderOptions;
    if !matches!(
        protocol,
        UpstreamProtocol::Responses
            | UpstreamProtocol::ChatCompletions
            | UpstreamProtocol::AnthropicMessages
    ) {
        return Err(error("该上游格式不适用于 Codex".into()));
    }
    if authentication.is_some_and(|scheme| {
        !matches!(
            scheme,
            AuthenticationScheme::Bearer | AuthenticationScheme::XApiKey
        )
    }) {
        return Err(error("Codex 上游认证只支持 Bearer 或 x-api-key".into()));
    }
    if connection.claude_native.is_some()
        || connection.claude_billing.is_some()
        || connection.claude_prompt_cache_key.is_some()
        || connection.claude_models_url.is_some()
        || connection.api_key_field.is_some()
    {
        return Err(error("Claude 专用连接选项不能进入 Codex 档案".into()));
    }
    if connection
        .auth_binding
        .as_ref()
        .is_some_and(|binding| binding.source == "managed_account")
        || connection.provider_type.as_deref() == Some("xai_oauth")
    {
        return Err(error(
            "Codex 第三方托管认证尚未实现，不能回退为静态 API 密钥；官方账号请使用独立绑定".into(),
        ));
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codex_authentication_is_independent_of_claude_and_survives_client_projection() {
        for scheme in [AuthenticationScheme::Bearer, AuthenticationScheme::XApiKey] {
            let mut draft = crate::codex_presets::prepare("codex-preset-02", "isolated-key")
                .unwrap()
                .draft;
            draft.authentication = Some(scheme);
            let file = draft.into_file("fixture".into(), 1);
            file.validate().unwrap();
            let profile = file.client_projection().into_profile(crate::AppKind::Codex);
            profile.validate().unwrap();
            assert_eq!(profile.authentication, Some(scheme));
        }
    }
    #[test]
    fn claude_only_authentication_is_not_accepted_by_the_codex_projection() {
        assert!(validate_connection(
            &ProviderConnectionOptions::default(),
            UpstreamProtocol::Responses,
            Some(AuthenticationScheme::XGoogApiKey)
        )
        .is_err());
    }
}
