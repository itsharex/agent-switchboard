use super::contracts::ProviderRequestConnection;
use crate::commands::error::CommandError;
use asb_core::contracts::{ProviderProfile, RouteMode, UpstreamProtocol};

impl TryFrom<&ProviderProfile> for ProviderRequestConnection {
    type Error = CommandError;

    fn try_from(profile: &ProviderProfile) -> Result<Self, Self::Error> {
        if profile.route_mode != RouteMode::Custom {
            return Err(CommandError::new(
                "provider-request-custom-only",
                "真实请求仅适用于使用 API 密钥的自定义供应商",
            ));
        }
        Ok(Self {
            base_url: profile.base_url.clone().ok_or_else(invalid)?,
            api_key: profile.api_key.clone(),
            upstream_protocol: profile.upstream_protocol.ok_or_else(invalid)?,
            responses_options: profile.responses_options,
            default_model: profile.model.clone(),
        })
    }
}

impl ProviderRequestConnection {
    pub(super) fn endpoint(&self) -> Result<String, CommandError> {
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
            || url.query().is_some()
            || url.fragment().is_some()
            || self.base_url.chars().any(char::is_control)
        {
            return Err(CommandError::new(
                "provider-request-endpoint-invalid",
                "真实请求要求服务地址不含 URL 凭据、查询参数、片段或控制字符",
            ));
        }
        asb_core::endpoint::upstream_endpoint(&self.base_url, self.upstream_protocol)
            .map_err(|message| CommandError::new("provider-request-endpoint-invalid", message))
    }
}

fn invalid() -> CommandError {
    CommandError::new(
        "provider-request-connection-invalid",
        "请检查服务地址、API 密钥、API 格式和 Responses 请求模式",
    )
}
