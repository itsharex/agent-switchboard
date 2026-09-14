//! Local managed accounts for Claude upstreams. Public views never contain credentials.

use asb_core::claude_auth::ClaudeAuthProvider;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeAccount {
    pub id: String,
    pub label: String,
    pub provider: ClaudeAuthProvider,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub upstream_account_id: Option<String>,
    pub github_domain: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AccountFile {
    pub schema_version: u32,
    pub defaults: BTreeMap<ClaudeAuthProvider, String>,
    pub accounts: Vec<ClaudeAccount>,
}

impl Default for AccountFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            defaults: BTreeMap::new(),
            accounts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeAccountView {
    pub id: String,
    pub label: String,
    pub provider: ClaudeAuthProvider,
    pub expires_at_ms: Option<i64>,
    pub upstream_account_id: Option<String>,
    pub github_domain: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeAccountsView {
    pub file_hash: String,
    pub accounts: Vec<ClaudeAccountView>,
}

impl ClaudeAccount {
    pub(super) fn validate(&self) -> Result<(), String> {
        validate_text(&self.id, "账号 ID", 128)?;
        validate_text(&self.label, "账号名称", 256)?;
        validate_text(&self.access_token, "访问凭据", 32768)?;
        if let Some(token) = &self.refresh_token {
            validate_text(token, "刷新凭据", 32768)?;
        }
        if let Some(id) = &self.upstream_account_id {
            validate_text(id, "上游账号 ID", 256)?;
        }
        if self.expires_at_ms.is_some_and(|expiry| expiry <= 0) {
            return Err("Claude 账号到期时间必须是 Unix 毫秒时间戳".into());
        }
        match self.provider {
            ClaudeAuthProvider::GithubCopilot => {
                if self.refresh_token.is_some()
                    || self.expires_at_ms.is_some()
                    || self.upstream_account_id.is_some()
                {
                    return Err(
                        "Copilot 账号应保存 GitHub 登录令牌，不应保存临时 Copilot 令牌".into(),
                    );
                }
                if let Some(domain) = &self.github_domain {
                    super::copilot::validate_domain(domain)?;
                }
            }
            ClaudeAuthProvider::CodexOauth | ClaudeAuthProvider::XaiOauth => {
                if self.github_domain.is_some() || self.expires_at_ms.is_none() {
                    return Err("Claude OAuth 账号需要到期时间，且不能设置 GitHub 域名".into());
                }
                if self.provider == ClaudeAuthProvider::CodexOauth
                    && self.upstream_account_id.is_none()
                {
                    return Err("Claude ChatGPT 账号缺少 workspace/account ID".into());
                }
            }
        }
        Ok(())
    }
}

pub(super) fn validate_text(value: &str, label: &str, limit: usize) -> Result<(), String> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(format!("Claude {label}格式无效"));
    }
    Ok(())
}

impl AccountFile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Claude 托管账号文件版本不受支持".into());
        }
        let mut ids = BTreeSet::new();
        for account in &self.accounts {
            account.validate()?;
            if !ids.insert(&account.id) {
                return Err("Claude 托管账号 ID 重复".into());
            }
        }
        for (provider, id) in &self.defaults {
            if !self
                .accounts
                .iter()
                .any(|account| &account.id == id && &account.provider == provider)
            {
                return Err("Claude 默认托管账号不存在或认证服务不匹配".into());
            }
        }
        Ok(())
    }

    pub fn view(&self, file_hash: String) -> ClaudeAccountsView {
        ClaudeAccountsView {
            file_hash,
            accounts: self
                .accounts
                .iter()
                .map(|account| ClaudeAccountView {
                    id: account.id.clone(),
                    label: account.label.clone(),
                    provider: account.provider,
                    expires_at_ms: account.expires_at_ms,
                    upstream_account_id: account.upstream_account_id.clone(),
                    github_domain: account.github_domain.clone(),
                    is_default: self.defaults.get(&account.provider) == Some(&account.id),
                })
                .collect(),
        }
    }
}
