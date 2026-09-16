//! The narrow Codex auth.json projection owned by direct Responses switches.

mod commit;
mod synchronize;
pub(crate) use commit::{commit_auth, restore_pair};
pub use synchronize::{synchronize_codex_auth, CodexAuthSyncOutcome};

use crate::{sha256_hex, RecoveryOutcome, SwitchError};
use asb_core::{AppKind, BackupRecord, RouteMode, SwitchPlan};
use serde_json::{Map, Value};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct AuthChange {
    pub target: PathBuf,
    pub before: String,
    pub before_hash: String,
    pub before_existed: bool,
    pub after: String,
    pub after_hash: String,
}

pub(crate) fn target_for(config_target: &Path) -> PathBuf {
    config_target.with_file_name("auth.json")
}

pub(crate) fn expected_action(plan: &SwitchPlan) -> Option<AuthAction<'_>> {
    if plan.app() != AppKind::Codex {
        return None;
    }
    if let Some(auth) = plan.codex_managed_auth() {
        return Some(AuthAction::Managed(auth));
    }
    if let Some(token) = plan.codex_gateway_credential() {
        return Some(AuthAction::Gateway(
            token,
            plan.codex_preserve_official_login(),
        ));
    }
    Some(if plan.profile.route_mode == RouteMode::Official {
        AuthAction::ClearApiKey
    } else {
        AuthAction::SetApiKey(&plan.profile.api_key, plan.codex_preserve_official_login())
    })
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AuthAction<'a> {
    SetApiKey(&'a str, bool),
    Gateway(&'a str, bool),
    ClearApiKey,
    Managed(&'a asb_core::contracts::CodexManagedAuth),
}

pub(crate) fn prepare<Io: crate::SwitchIo>(
    io: &Io,
    config_target: &Path,
    action: Option<AuthAction<'_>>,
) -> Result<Option<AuthChange>, SwitchError> {
    let Some(action) = action else {
        return Ok(None);
    };
    let target = target_for(config_target);
    let (before, before_existed) = match io.read_file(&target) {
        Ok(text) => (text, true),
        Err(error) if error.kind() == ErrorKind::NotFound => (String::new(), false),
        Err(error) => return Err(read_error(error.to_string())),
    };
    let after = match render(&before, before_existed, action) {
        Ok(after) => after,
        // Official switching must never rewrite or manufacture a broken
        // credential cache. A later login flow remains the owner of repair.
        Err(_error) if matches!(action, AuthAction::ClearApiKey) => return Ok(None),
        Err(error) => return Err(error),
    };
    if after == before {
        return Ok(None);
    }
    Ok(Some(AuthChange {
        target,
        before_hash: sha256_hex(&before),
        after_hash: sha256_hex(&after),
        before,
        before_existed,
        after,
    }))
}

fn render(before: &str, existed: bool, action: AuthAction<'_>) -> Result<String, SwitchError> {
    let mut root = if existed {
        serde_json::from_str::<Value>(before)
            .map_err(|_| invalid_error("Codex auth.json 不是有效 JSON"))?
            .as_object()
            .cloned()
            .ok_or_else(|| invalid_error("Codex auth.json 顶层必须是 JSON 对象"))?
    } else {
        Map::new()
    };
    match action {
        AuthAction::Managed(auth) => render_managed(&mut root, auth)?,
        AuthAction::Gateway(_, true) if has_oauth(&root) => return Ok(before.to_string()),
        AuthAction::SetApiKey(key, preserve) | AuthAction::Gateway(key, preserve) => {
            if key.trim().is_empty() {
                return Err(invalid_error("Codex 直连供应商缺少 API 密钥"));
            }
            root.insert("OPENAI_API_KEY".into(), Value::String(key.into()));
            root.insert("auth_mode".into(), Value::String("apikey".into()));
            root.remove("asb_managed_account");
            if !preserve {
                root.remove("tokens");
                root.remove("last_refresh");
            }
        }
        AuthAction::ClearApiKey => {
            if !root.contains_key("OPENAI_API_KEY")
                && root.get("auth_mode").and_then(Value::as_str) != Some("apikey")
            {
                return Ok(before.to_string());
            }
            root.insert("OPENAI_API_KEY".into(), Value::Null);
            root.insert("auth_mode".into(), Value::String("chatgpt".into()));
        }
    }
    serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|_| invalid_error("Codex auth.json 无法序列化"))
}

pub(crate) fn verify_snapshot<Io: crate::SwitchIo>(
    io: &Io,
    change: &AuthChange,
) -> Result<(), SwitchError> {
    let (current, existed) = match io.read_file(&change.target) {
        Ok(text) => (text, true),
        Err(error) if error.kind() == ErrorKind::NotFound => (String::new(), false),
        Err(error) => return Err(read_error(error.to_string())),
    };
    if existed == change.before_existed && sha256_hex(&current) == change.before_hash {
        return Ok(());
    }
    Err(SwitchError::ExternalChange {
        expected_hash: change.before_hash.clone(),
        found_hash: sha256_hex(&current),
    })
}

pub(crate) fn backup<Io: crate::SwitchIo>(
    io: &Io,
    change: &AuthChange,
    config_backup: &BackupRecord,
    backup_dir: &Path,
    timestamp: &str,
) -> Result<BackupRecord, SwitchError> {
    io.ensure_dir(backup_dir)
        .map_err(|error| commit_error("backup-dir", error.to_string()))?;
    let path = backup_dir.join(format!("auth.json.{timestamp}.bak"));
    io.write_new_file(&path, &change.before)
        .map_err(|error| commit_error("backup", error.to_string()))?;
    io.set_mode(&path, 0o600)
        .map_err(|error| commit_error("auth-backup-permissions", error.to_string()))?;
    let read_back = io
        .read_file(&path)
        .map_err(|error| commit_error("backup-verify", error.to_string()))?;
    if read_back != change.before {
        return Err(commit_error("backup-verify", "认证备份回读内容不匹配"));
    }
    let record = BackupRecord {
        id: format!(
            "{}-auth-{timestamp}",
            &change.before_hash[..12.min(change.before_hash.len())]
        ),
        app: AppKind::Codex,
        target_path: change.target.to_string_lossy().into_owned(),
        backup_path: path.to_string_lossy().into_owned(),
        created_at: io.now_rfc3339(),
        content_hash: change.before_hash.clone(),
        target_existed: change.before_existed,
        linked_backup_id: Some(config_backup.id.clone()),
        reason: "codex-auth-projection".into(),
    };
    crate::executor::write_backup_metadata(io, &record, "backup-meta")?;
    Ok(record)
}

fn read_error(message: String) -> SwitchError {
    SwitchError::PlanRejected {
        message,
        line: None,
    }
}

fn invalid_error(message: &str) -> SwitchError {
    SwitchError::PlanRejected {
        message: message.into(),
        line: None,
    }
}

fn commit_error(stage: &'static str, message: impl Into<String>) -> SwitchError {
    SwitchError::CommitFailed {
        stage,
        message: message.into(),
        recovery: RecoveryOutcome::NotNeeded,
    }
}

fn render_managed(
    root: &mut Map<String, Value>,
    auth: &asb_core::contracts::CodexManagedAuth,
) -> Result<(), SwitchError> {
    validate_managed(auth)?;
    root.insert("OPENAI_API_KEY".into(), Value::Null);
    root.insert("auth_mode".into(), Value::String("chatgpt".into()));
    merge_tokens(root, auth);
    root.insert(
        "last_refresh".into(),
        Value::String(auth.last_refresh.clone()),
    );
    root.insert(
        "asb_managed_account".into(),
        serde_json::json!({
            "id": auth.managed_id, "generation": auth.generation,
        }),
    );
    Ok(())
}

fn has_oauth(root: &Map<String, Value>) -> bool {
    root.get("auth_mode").and_then(Value::as_str) == Some("chatgpt")
        && ["id_token", "access_token", "refresh_token"]
            .iter()
            .all(|key| {
                root.get("tokens")
                    .and_then(|v| v.get(*key))
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
            })
}

/// Restoring a historical configuration must never put an older managed token
/// generation back into the native credential store.
pub(crate) fn validate_restore_generation(
    current: &str,
    candidate: &str,
) -> Result<(), SwitchError> {
    let (Ok(current), Ok(candidate)) = (
        serde_json::from_str::<Value>(current),
        serde_json::from_str::<Value>(candidate),
    ) else {
        return Ok(());
    };
    let id = |value: &Value| {
        value
            .pointer("/asb_managed_account/id")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    if id(&current).is_none() || id(&current) != id(&candidate) {
        return Ok(());
    }
    let generation = |value: &Value| {
        value
            .pointer("/asb_managed_account/generation")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let refreshed = |value: &Value| {
        value
            .get("last_refresh")
            .and_then(Value::as_str)
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
    };
    if generation(&current) > generation(&candidate) || refreshed(&current) > refreshed(&candidate)
    {
        return Err(invalid_error(
            "备份中的 Codex 账号令牌代际早于当前登录；请重新应用所需供应商，不要恢复旧令牌",
        ));
    }
    Ok(())
}

/// The auth transaction owns only the file store. A different
/// `cli_auth_credentials_store` is a deliberate user choice this transaction
/// cannot honour, so the switch is refused rather than writing a file store
/// behind an unreadable selection.
pub(crate) fn validate_storage(config: &str, plan: &SwitchPlan) -> Result<(), SwitchError> {
    if expected_action(plan).is_none() {
        return Ok(());
    }
    let document = config
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| invalid_error("Codex 配置格式无效"))?;
    if document
        .get("cli_auth_credentials_store")
        .is_some_and(|value| value.as_str() != Some("file"))
    {
        return Err(invalid_error("此认证事务只写文件存储，不能覆盖 keyring/auto/ephemeral 的认证选择；请使用原生登录或明确选择 file 后重试"));
    }
    Ok(())
}

/// Public seam for the integration suite: proves the storage guard in
/// isolation, independent of the executor's earlier hash checks.
pub fn validate_codex_auth_storage(config: &str, plan: &SwitchPlan) -> Result<(), SwitchError> {
    validate_storage(config, plan)
}

fn validate_managed(auth: &asb_core::contracts::CodexManagedAuth) -> Result<(), SwitchError> {    if [
        &auth.managed_id,
        &auth.account_id,
        &auth.subject,
        &auth.id_token,
        &auth.access_token,
        &auth.refresh_token,
    ]
    .iter()
    .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        || auth.generation == 0
    {
        return Err(invalid_error("Codex 托管认证投影不完整"));
    }
    Ok(())
}

fn token_value(auth: &asb_core::contracts::CodexManagedAuth) -> Value {
    serde_json::json!({"id_token":auth.id_token,"access_token":auth.access_token,"refresh_token":auth.refresh_token,"account_id":auth.account_id})
}

fn merge_tokens(root: &mut Map<String, Value>, auth: &asb_core::contracts::CodexManagedAuth) {
    let mut tokens = root
        .get("tokens")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in token_value(auth).as_object().expect("token object") {
        tokens.insert(key.clone(), value.clone());
    }
    root.insert("tokens".into(), Value::Object(tokens));
}
