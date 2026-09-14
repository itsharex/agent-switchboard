//! Claude managed-subscription observations, separate from request metering and native CLI quotas.

mod grok;
mod json;
use super::{http, store, ClaudeAuth};
use asb_core::{
    claude_auth::ClaudeAuthProvider,
    contracts::{ProviderAuthBinding, ProviderConnectionOptions},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeAccountQuota {
    pub provider: ClaudeAuthProvider,
    pub account_id: String,
    pub plan: Option<String>,
    pub windows: Vec<ClaudeQuotaWindow>,
    pub checked_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeQuotaWindow {
    pub id: String,
    pub label: String,
    pub used_percent: Option<f64>,
    pub remaining: Option<f64>,
    pub limit: Option<f64>,
    pub unlimited: bool,
    pub unit: &'static str,
    pub resets_at_ms: Option<i64>,
}

impl ClaudeAuth {
    pub(crate) fn quota(
        &self,
        provider: ClaudeAuthProvider,
        selected: Option<&str>,
    ) -> Result<ClaudeAccountQuota, String> {
        let (file, _) = store::load(&self.root)?;
        let id = selected
            .or_else(|| file.defaults.get(&provider).map(String::as_str))
            .ok_or("Claude 认证服务没有默认账号，请先登录或选择账号")?;
        let account = file
            .accounts
            .iter()
            .find(|account| account.id == id && account.provider == provider)
            .ok_or("Claude 指定账号不存在或认证服务不匹配；不会回退到其他账号")?;
        let now = chrono::Utc::now().timestamp_millis();
        let (plan, windows) = if provider == ClaudeAuthProvider::GithubCopilot {
            copilot_quota(self, account)?
        } else {
            let options = selection(provider, Some(id));
            let resolved = self.resolve(&options)?.ok_or("Claude 托管认证不可用")?;
            match provider {
                ClaudeAuthProvider::CodexOauth => {
                    let url = "https://chatgpt.com/backend-api/wham/usage".to_string();
                    #[cfg(test)]
                    let url = self
                        .endpoints
                        .as_ref()
                        .map(|endpoints| format!("{}/quota", endpoints.upstream))
                        .unwrap_or(url);
                    let authorization = format!("Bearer {}", resolved.access_token);
                    let value = http::json_request(
                        reqwest::Method::GET,
                        &url,
                        &[
                            ("authorization", &authorization),
                            (
                                "chatgpt-account-id",
                                resolved.upstream_account_id.as_deref().unwrap_or(""),
                            ),
                            ("accept", "application/json"),
                        ],
                        Vec::new(),
                    )?;
                    json::chatgpt(&value)?
                }
                ClaudeAuthProvider::XaiOauth => {
                    let url = grok::ENDPOINT.to_string();
                    #[cfg(test)]
                    let url = self
                        .endpoints
                        .as_ref()
                        .map(|endpoints| format!("{}/quota", endpoints.upstream))
                        .unwrap_or(url);
                    (
                        Some("SuperGrok".into()),
                        vec![grok::fetch(&url, &resolved.access_token, now)?],
                    )
                }
                ClaudeAuthProvider::GithubCopilot => unreachable!(),
            }
        };
        Ok(ClaudeAccountQuota {
            provider,
            account_id: account.id.clone(),
            plan,
            windows,
            checked_at_ms: now,
        })
    }
}

pub(crate) fn selection(
    provider: ClaudeAuthProvider,
    id: Option<&str>,
) -> ProviderConnectionOptions {
    let kind = match provider {
        ClaudeAuthProvider::GithubCopilot => "github_copilot",
        ClaudeAuthProvider::CodexOauth => "codex_oauth",
        ClaudeAuthProvider::XaiOauth => "xai_oauth",
    };
    ProviderConnectionOptions {
        provider_type: Some(kind.into()),
        auth_binding: Some(ProviderAuthBinding {
            source: "managed_account".into(),
            auth_provider: Some(kind.into()),
            account_id: id.map(str::to_string),
        }),
        ..Default::default()
    }
}

fn copilot_quota(
    _auth: &ClaudeAuth,
    account: &super::ClaudeAccount,
) -> Result<(Option<String>, Vec<ClaudeQuotaWindow>), String> {
    let base = super::copilot::api_base(account);
    #[cfg(test)]
    let base = _auth
        .endpoints
        .as_ref()
        .map(|endpoints| endpoints.github_api.clone())
        .unwrap_or(base);
    let authorization = format!("token {}", account.access_token);
    let value = http::json_request(
        reqwest::Method::GET,
        &format!("{base}/copilot_internal/user"),
        &[
            ("authorization", &authorization),
            ("accept", "application/json"),
        ],
        Vec::new(),
    )?;
    json::copilot(&value)
}
