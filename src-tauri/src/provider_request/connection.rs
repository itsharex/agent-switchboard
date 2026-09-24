use super::contracts::ProviderRequestConnection;
use crate::commands::error::CommandError;
use asb_core::contracts::{ProviderProfile, RouteMode, UpstreamProtocol};

impl TryFrom<&ProviderProfile> for ProviderRequestConnection {
    type Error = CommandError;

    fn try_from(profile: &ProviderProfile) -> Result<Self, Self::Error> {
        if profile.route_mode != RouteMode::Custom {
            return Err(CommandError::keyed(
                "provider-request-custom-only",
                "errors.misc.providerRequestCustomOnly",
                "真实请求仅适用于自定义供应商（API 密钥或托管账号）",
            ));
        }
        Ok(Self {
            app: profile.app,
            claude_account: None,
            base_url: profile.base_url.clone().ok_or_else(invalid)?,
            connection: profile.connection.clone(),
            api_key: profile.api_key.clone(),
            authentication: profile.authentication,
            upstream_protocol: profile.upstream_protocol.ok_or_else(invalid)?,
            responses_options: profile.responses_options,
            default_model: profile.model.clone(),
        })
    }
}

impl ProviderRequestConnection {
    pub(super) fn endpoint(&self, model: Option<&str>) -> Result<String, CommandError> {
        if self.app != asb_core::AppKind::Claude
            && self.upstream_protocol == UpstreamProtocol::GeminiGenerateContent
        {
            return Err(invalid());
        }
        if self.api_key.trim().is_empty()
            || reqwest::header::HeaderValue::from_str(&self.api_key).is_err()
            || (self.upstream_protocol == UpstreamProtocol::Responses)
                != self.responses_options.is_some()
        {
            return Err(invalid());
        }
        // The returned endpoint is renderer-visible; never allow URL credentials.
        let url = reqwest::Url::parse(&self.base_url).map_err(|_| invalid())?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || self.base_url.chars().any(char::is_control)
        {
            return Err(CommandError::keyed(
                "provider-request-endpoint-invalid",
                "errors.misc.endpointUrlCredentialsForbidden",
                "真实请求要求服务地址不含 URL 凭据、片段或控制字符",
            ));
        }
        if self.upstream_protocol == UpstreamProtocol::GeminiGenerateContent {
            return match model {
                Some(model) => asb_core::claude_gemini::request_endpoint(
                    &self.base_url,
                    self.connection.is_full_url,
                    model,
                    false,
                ),
                None => asb_core::claude_gemini::request_preview(
                    &self.base_url,
                    self.connection.is_full_url,
                    self.default_model.as_deref(),
                ),
            }
            .map_err(|message| CommandError::new("provider-request-endpoint-invalid", message));
        }
        asb_core::endpoint::upstream_endpoint_with_options(
            &self.base_url,
            self.upstream_protocol,
            self.connection.is_full_url,
        )
        .map_err(|message| CommandError::new("provider-request-endpoint-invalid", message))
    }
}

fn invalid() -> CommandError {
    CommandError::keyed(
        "provider-request-connection-invalid",
        "errors.misc.providerRequestConnectionInvalid",
        "请检查服务地址、API 密钥、API 格式和 Responses 请求模式",
    )
}
