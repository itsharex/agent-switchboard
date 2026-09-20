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
        || connection.api_key_field.is_some()
    {
        return Err(error("Claude 专用连接选项不能进入 Codex 档案".into()));
    }
    connection.validate_models_url().map_err(|message| error(message))?;
    if connection
        .auth_binding
        .as_ref()
        .is_some_and(|binding| binding.source == "managed_account")
    {
        return Err(error(
            "Codex 第三方托管认证尚未实现，不能回退为静态 API 密钥；官方账号请使用独立绑定".into(),
        ));
    }
    if connection.provider_type.as_deref() == Some("xai_oauth") {
        if protocol != UpstreamProtocol::Responses {
            return Err(error("xAI 托管账号只能以原生 Responses 上游使用".into()));
        }
        if authentication.is_some_and(|scheme| scheme != AuthenticationScheme::Bearer) {
            return Err(error("xAI 托管账号固定使用 Bearer 认证".into()));
        }
        // Managed xAI cards carry no static credential: the gateway resolves
        // the account token per request.
        return Ok(true);
    }
    connection
        .validate_request_overrides()
        .map_err(|message| error(message))?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> crate::contracts::CodexProviderDraft {
        use crate::contracts::{
            CodexCatalogEntry, CodexEndpoint, CodexProviderDraft, CodexUpstream,
            ResponsesRequestMode, DEFAULT_CODEX_CAPABILITIES,
        };
        CodexProviderDraft {
            name: "Relay".into(),
            endpoint: CodexEndpoint("https://relay.example/v1".into()),
            api_key: "fixture-key".into(),
            authentication: None,
            connection: Default::default(),
            upstream: CodexUpstream::Responses,
            request_mode: ResponsesRequestMode::Standard,
            default_model: "relay-model".into(),
            catalog: vec![CodexCatalogEntry::default_entry("relay-model")],
            model_routes: Vec::new(),
            subagent_route: None,
            capabilities: DEFAULT_CODEX_CAPABILITIES,
            parameters: crate::ownership::default_provider_parameters(crate::AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
        }
    }

    #[test]
    fn codex_authentication_is_independent_of_claude_and_survives_client_projection() {
        for scheme in [AuthenticationScheme::Bearer, AuthenticationScheme::XApiKey] {
            let mut draft = draft();
            draft.authentication = Some(scheme);
            let file = draft.into_file("fixture".into(), 1);
            file.validate().unwrap();
            let profile = file.client_projection().into_profile(crate::AppKind::Codex);
            profile.validate().unwrap();
            assert_eq!(profile.authentication, Some(scheme));
        }
    }

    #[test]
    fn request_overrides_are_validated_at_save_time() {
        use crate::contracts::{LocalProxyRequestOverrides, ProviderConnectionOptions};
        use std::collections::BTreeMap;
        let mut headers = BTreeMap::new();
        headers.insert("x-provider-tag".to_string(), "override".to_string());
        let mut options = ProviderConnectionOptions {
            custom_user_agent: Some("fixture/1.0".to_string()),
            local_proxy_request_overrides: Some(LocalProxyRequestOverrides {
                headers,
                body: serde_json::json!({"model": "m"}),
            }),
            ..Default::default()
        };
        assert!(options.validate_request_overrides().is_ok());
        assert!(
            validate_connection(&options, UpstreamProtocol::ChatCompletions, None).is_ok(),
            "valid overrides pass the codex connection validation"
        );
        let mut protected = options.clone();
        protected
            .local_proxy_request_overrides
            .as_mut()
            .unwrap()
            .headers
            .insert("authorization".to_string(), "Bearer blocked".to_string());
        assert!(protected.validate_request_overrides().is_err());
        let mut bad_name = options.clone();
        bad_name
            .local_proxy_request_overrides
            .as_mut()
            .unwrap()
            .headers
            .insert("bad name".to_string(), "v".to_string());
        assert!(bad_name.validate_request_overrides().is_err());
        let mut bad_body = options.clone();
        bad_body
            .local_proxy_request_overrides
            .as_mut()
            .unwrap()
            .body = serde_json::json!([1, 2, 3]);
        assert!(bad_body.validate_request_overrides().is_err());
        let mut bad_ua = options;
        bad_ua.custom_user_agent = Some("ua\u{7}control".to_string());
        assert!(bad_ua.validate_request_overrides().is_err());
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
