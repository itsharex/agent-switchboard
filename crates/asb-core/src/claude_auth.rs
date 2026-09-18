//! Claude managed authentication selection. No Codex client policy enters this contract.

use crate::contracts::{ProviderConnectionOptions, UpstreamProtocol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeAuthProvider {
    GithubCopilot,
    CodexOauth,
    XaiOauth,
}

impl ClaudeAuthProvider {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "github_copilot" => Ok(Self::GithubCopilot),
            "codex_oauth" => Ok(Self::CodexOauth),
            "xai_oauth" => Ok(Self::XaiOauth),
            _ => Err(format!("Claude 不支持此托管认证服务：{value}")),
        }
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::GithubCopilot => "https://api.githubcopilot.com",
            Self::CodexOauth => "https://chatgpt.com/backend-api/codex",
            Self::XaiOauth => "https://api.x.ai/v1",
        }
    }

    pub fn default_protocol(self) -> UpstreamProtocol {
        match self {
            Self::GithubCopilot => UpstreamProtocol::AnthropicMessages,
            Self::CodexOauth | Self::XaiOauth => UpstreamProtocol::Responses,
        }
    }

    pub fn validate_protocol(self, protocol: UpstreamProtocol) -> Result<(), String> {
        if self != Self::GithubCopilot && protocol != UpstreamProtocol::Responses {
            return Err("Claude 的 ChatGPT / xAI 托管认证必须使用 Responses 协议".into());
        }
        Ok(())
    }
}

/// An explicit account id stays pinned; None follows this auth service's local default.
pub fn managed_auth(
    options: &ProviderConnectionOptions,
) -> Result<Option<(ClaudeAuthProvider, Option<&str>)>, String> {
    let selected = select_account(options)?;
    if selected.is_some()
        && (options.is_full_url
            || !options.custom_endpoints.is_empty()
            || options.models_url.is_some())
    {
        return Err("Claude 托管账号的请求与模型端点由授权服务确定，不能使用完整 URL、自定义端点或模型地址覆盖".into());
    }
    Ok(selected)
}

fn select_account(
    options: &ProviderConnectionOptions,
) -> Result<Option<(ClaudeAuthProvider, Option<&str>)>, String> {
    let kind = options
        .provider_type
        .as_deref()
        .map(ClaudeAuthProvider::parse)
        .transpose()?;
    let Some(binding) = &options.auth_binding else {
        return Ok(kind.map(|kind| (kind, None)));
    };
    match binding.source.as_str() {
        "provider_config" => {
            if binding.auth_provider.is_some() || binding.account_id.is_some() {
                return Err("provider_config 认证不能同时指定托管账号".into());
            }
            Ok(kind.map(|kind| (kind, None)))
        }
        "managed_account" => {
            let bound = binding
                .auth_provider
                .as_deref()
                .ok_or("Claude 托管认证缺少 authProvider")
                .and_then(|value| {
                    ClaudeAuthProvider::parse(value).map_err(|_| "Claude 托管认证服务无效")
                })?;
            if kind.is_some_and(|kind| kind != bound) {
                return Err("Claude providerType 与 authBinding.authProvider 不一致".into());
            }
            if binding
                .account_id
                .as_deref()
                .is_some_and(|id| id.trim().is_empty() || id.chars().any(char::is_control))
            {
                return Err("Claude 托管账号 ID 无效".into());
            }
            Ok(Some((bound, binding.account_id.as_deref())))
        }
        _ => Err("Claude 认证来源必须是 provider_config 或 managed_account".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ProviderAuthBinding;

    #[test]
    fn explicit_accounts_never_fall_back_to_a_different_auth_service() {
        let mut options = ProviderConnectionOptions {
            provider_type: Some("github_copilot".into()),
            auth_binding: Some(ProviderAuthBinding {
                source: "managed_account".into(),
                auth_provider: Some("github_copilot".into()),
                account_id: Some("alice".into()),
            }),
            ..Default::default()
        };
        assert_eq!(
            managed_auth(&options).unwrap(),
            Some((ClaudeAuthProvider::GithubCopilot, Some("alice")))
        );
        options.auth_binding.as_mut().unwrap().auth_provider = Some("codex_oauth".into());
        assert!(managed_auth(&options).is_err());
    }

    #[test]
    fn managed_types_require_gateway_and_provider_config_does_not_invent_an_account() {
        let options = ProviderConnectionOptions {
            provider_type: Some("codex_oauth".into()),
            ..Default::default()
        };
        assert!(options.requires_gateway(None));
        assert!(ClaudeAuthProvider::CodexOauth
            .validate_protocol(UpstreamProtocol::AnthropicMessages)
            .is_err());
        assert_eq!(
            managed_auth(&ProviderConnectionOptions::default()).unwrap(),
            None
        );
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    #[test]
    fn managed_endpoint_overrides_are_rejected_instead_of_silently_ignored() {
        let mut options = ProviderConnectionOptions {
            provider_type: Some("codex_oauth".into()),
            is_full_url: true,
            ..Default::default()
        };
        assert!(managed_auth(&options).is_err());
        options.is_full_url = false;
        options.models_url = Some("https://other.example/models".into());
        assert!(managed_auth(&options).is_err());
        options.models_url = None;
        options.custom_user_agent = Some("ASB custom client".into());
        assert!(managed_auth(&options).is_ok());
    }
}
