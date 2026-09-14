//! Claude upstream device-login endpoints, isolated from native client logins.

use super::super::{copilot, http::json_request, oauth};
use asb_core::claude_auth::ClaudeAuthProvider;

#[derive(Clone)]
pub(crate) struct DeviceEndpoints {
    pub start: String,
    pub poll: String,
    pub token: String,
    pub user: String,
    pub verification: String,
    pub client_id: String,
    pub scope: String,
}

impl DeviceEndpoints {
    pub(super) fn discover(provider: ClaudeAuthProvider, domain: &str) -> Result<Self, String> {
        match provider {
            ClaudeAuthProvider::GithubCopilot => {
                copilot::validate_domain(domain)?;
                let api = if domain == "github.com" {
                    "https://api.github.com".into()
                } else {
                    format!("https://{domain}/api/v3")
                };
                Ok(Self {
                    start: format!("https://{domain}/login/device/code"),
                    poll: format!("https://{domain}/login/oauth/access_token"),
                    token: String::new(),
                    user: format!("{api}/user"),
                    verification: format!("https://{domain}/login/device"),
                    client_id: if domain == "github.com" {
                        "Iv1.b507a08c87ecfe98"
                    } else {
                        "Ov23li8tweQw6odWQebz"
                    }
                    .into(),
                    scope: "read:user".into(),
                })
            }
            ClaudeAuthProvider::CodexOauth => Ok(Self {
                start: "https://auth.openai.com/api/accounts/deviceauth/usercode".into(),
                poll: "https://auth.openai.com/api/accounts/deviceauth/token".into(),
                token: oauth::CHATGPT_TOKEN_URL.into(),
                user: String::new(),
                verification: "https://auth.openai.com/codex/device".into(),
                client_id: oauth::CHATGPT_CLIENT_ID.into(),
                scope: String::new(),
            }),
            ClaudeAuthProvider::XaiOauth => Self::xai(oauth::XAI_DISCOVERY_URL),
        }
    }

    pub(super) fn xai(url: &str) -> Result<Self, String> {
        let value = json_request(
            reqwest::Method::GET,
            url,
            &[("accept", "application/json")],
            Vec::new(),
        )?;
        if value["issuer"] != "https://auth.x.ai" {
            return Err("xAI 认证服务 issuer 不匹配".into());
        }
        let endpoint = |key: &str| -> Result<String, String> {
            let raw = value[key]
                .as_str()
                .ok_or_else(|| format!("xAI 认证发现缺少 {key}"))?;
            oauth::validate_xai_endpoint(raw)?;
            Ok(raw.into())
        };
        Ok(Self {
            start: endpoint("device_authorization_endpoint")?,
            poll: endpoint("token_endpoint")?,
            token: endpoint("token_endpoint")?,
            user: String::new(),
            verification: "https://auth.x.ai".into(),
            client_id: oauth::XAI_CLIENT_ID.into(),
            scope: oauth::XAI_SCOPE.into(),
        })
    }
}
