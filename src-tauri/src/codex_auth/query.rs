use super::manager;
use asb_core::contracts::{CodexOfficialQuota, CodexOfficialQuotaStatus};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountQuota {
    pub account_id: String,
    pub quota: CodexOfficialQuota,
    pub warning: Option<String>,
}

pub(crate) fn official_models_document(
    root: &Path,
    managed_id: &str,
    auth_path: &Path,
) -> Result<String, String> {
    let account = manager::valid_account(root, Some(managed_id), auth_path)?;
    let (status, body) = get(
        &account,
        &format!(
            "{}/models?client_version=0.152.1",
            super::OFFICIAL_CODEX_BASE
        ),
    )?;
    if status != 200 {
        return Err(format!("Codex 官方模型查询失败（HTTP {status}）"));
    }
    Ok(body)
}

pub(crate) fn quota(
    root: &Path,
    id: Option<&str>,
    auth_path: &Path,
) -> Result<AccountQuota, String> {
    let account = manager::valid_account(root, id, auth_path)?;
    static CACHE: OnceLock<Mutex<HashMap<(std::path::PathBuf, String), CodexOfficialQuota>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (root.to_path_buf(), account.id.clone());
    let response = get(&account, "https://chatgpt.com/backend-api/wham/usage");
    let mut quota = match response {
        Ok((status, body)) => crate::codex_official_quota::quota_from_http_response(
            status,
            &body,
            chrono::Utc::now().to_rfc3339(),
        ),
        Err(_) => crate::codex_official_quota::quota_from_http_response(
            503,
            "",
            chrono::Utc::now().to_rfc3339(),
        ),
    };
    let mut warning = account.native_sync_error.clone();
    if quota.status == CodexOfficialQuotaStatus::Available {
        match super::quota_history::record(root, &account.id, &quota) {
            Ok(reset) => quota.last_reset = reset,
            Err(error) => {
                warning =
                    Some(warning.map_or_else(|| error.clone(), |prior| format!("{prior}；{error}")))
            }
        }
    }
    let mut cache = cache.lock().map_err(|_| "Codex 账号额度缓存不可用")?;
    if quota.status == CodexOfficialQuotaStatus::Available {
        cache.insert(key, quota.clone());
    } else if quota.status == CodexOfficialQuotaStatus::Unavailable {
        if let Some(previous) = cache.get(&key) {
            quota = previous.clone();
            quota.stale = true;
        }
    } else {
        cache.remove(&key);
    }
    Ok(AccountQuota {
        account_id: account.id,
        quota,
        warning,
    })
}
fn get(account: &super::contracts::Account, url: &str) -> Result<(u16, String), String> {
    let headers = format!("Authorization: Bearer {}\r\nChatGPT-Account-Id: {}\r\nAccept: application/json\r\nOriginator: codex_cli_rs\r\nVersion: 0.152.1\r\n", account.tokens.access_token, account.identity.account_id);
    crate::probe::http_request("GET", url, &headers, &[])
        .map_err(|_| "Codex 账号查询网络失败，请稍后重试".into())
}
