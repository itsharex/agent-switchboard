//! Refresh a shared OAuth bundle without changing its native auth mode or route.
use super::*;
use crate::{lockfile, SwitchIo};
use asb_core::contracts::CodexManagedAuth;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexAuthSyncOutcome {
    Synchronized,
    Detached,
}

pub fn synchronize_codex_auth<Io: SwitchIo>(
    io: &Io,
    config_target: &Path,
    backup_dir: &Path,
    expected_hash: &str,
    expected_refresh_hash: &str,
    auth: &CodexManagedAuth,
) -> Result<CodexAuthSyncOutcome, SwitchError> {
    validate_managed(auth)?;
    if [expected_hash, expected_refresh_hash]
        .iter()
        .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(invalid_error("Codex 认证同步版本无效"));
    }
    match lockfile::acquire(io, config_target, "agent-switchboard-auth-refresh") {
        lockfile::AcquireOutcome::Acquired => {}
        lockfile::AcquireOutcome::Busy(status) => {
            return Err(SwitchError::BlockedByLock { status })
        }
    }
    let result = synchronize_locked(
        io,
        config_target,
        backup_dir,
        expected_hash,
        expected_refresh_hash,
        auth,
    );
    match lockfile::release(io, config_target) {
        Ok(()) => result,
        Err(message) => Err(commit_error("auth-refresh-lock-release", message)),
    }
}
fn synchronize_locked<Io: SwitchIo>(
    io: &Io,
    config_target: &Path,
    backup_dir: &Path,
    expected_hash: &str,
    expected_refresh_hash: &str,
    auth: &CodexManagedAuth,
) -> Result<CodexAuthSyncOutcome, SwitchError> {
    let target = target_for(config_target);
    let before = match io.read_file(&target) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(CodexAuthSyncOutcome::Detached),
        Err(e) => return Err(read_error(e.to_string())),
    };
    let mut value: Value = serde_json::from_str(&before)
        .map_err(|_| invalid_error("Codex 原生登录格式无效，未同步令牌"))?;
    let Some(root) = value.as_object_mut() else {
        return Err(invalid_error("Codex 原生登录不是 JSON 对象"));
    };
    if token_fields_match(root, auth) {
        return Ok(CodexAuthSyncOutcome::Synchronized);
    }
    let refresh = root
        .get("tokens")
        .and_then(|v| v.get("refresh_token"))
        .and_then(Value::as_str);
    if !refresh.is_some_and(|token| sha256_hex(token) == expected_refresh_hash) {
        return Ok(CodexAuthSyncOutcome::Detached);
    }
    if sha256_hex(&before) != expected_hash {
        return Err(SwitchError::ExternalChange {
            expected_hash: expected_hash.into(),
            found_hash: sha256_hex(&before),
        });
    }
    validate_managed(auth)?;
    merge_tokens(root, auth);
    root.insert(
        "last_refresh".into(),
        Value::String(auth.last_refresh.clone()),
    );
    if root
        .get("asb_managed_account")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        == Some(auth.managed_id.as_str())
    {
        root.insert(
            "asb_managed_account".into(),
            serde_json::json!({"id":auth.managed_id,"generation":auth.generation}),
        );
    }
    let after = serde_json::to_string_pretty(&value)
        .map_err(|_| invalid_error("Codex 认证同步无法序列化"))?;
    let change = AuthChange {
        target,
        before_hash: sha256_hex(&before),
        before,
        before_existed: true,
        after_hash: sha256_hex(&after),
        after,
    };
    let backup = refresh_backup(io, backup_dir, &change)?;
    if let Err(error) = commit_auth(io, &change) {
        let recovery = crate::restore::restore_backup_if_unchanged(
            io,
            &change.target,
            &backup,
            &change.after,
            true,
        );
        return Err(SwitchError::CommitFailed {
            stage: "auth-refresh",
            message: error.to_string(),
            recovery,
        });
    }
    Ok(CodexAuthSyncOutcome::Synchronized)
}
fn refresh_backup<Io: SwitchIo>(
    io: &Io,
    directory: &Path,
    change: &AuthChange,
) -> Result<BackupRecord, SwitchError> {
    io.ensure_dir(directory)
        .map_err(|e| commit_error("auth-refresh-backup-dir", e.to_string()))?;
    let timestamp = crate::executor::timestamp_name(io);
    let path = directory.join(format!("auth-refresh.{timestamp}.bak"));
    io.write_new_file(&path, &change.before)
        .and_then(|_| io.set_mode(&path, 0o600))
        .map_err(|e| commit_error("auth-refresh-backup", e.to_string()))?;
    if io.read_file(&path).ok().as_deref() != Some(&change.before) {
        return Err(commit_error(
            "auth-refresh-backup-verify",
            "认证刷新备份回读不一致",
        ));
    }
    let backup = BackupRecord {
        id: format!("auth-refresh-{timestamp}"),
        app: AppKind::Codex,
        target_path: change.target.to_string_lossy().into_owned(),
        backup_path: path.to_string_lossy().into_owned(),
        created_at: io.now_rfc3339(),
        content_hash: change.before_hash.clone(),
        target_existed: true,
        linked_backup_id: None,
        reason: "codex-auth-refresh".into(),
    };
    crate::executor::write_backup_metadata(io, &backup, "auth-refresh-backup-meta")?;
    Ok(backup)
}

fn token_fields_match(root: &Map<String, Value>, auth: &CodexManagedAuth) -> bool {
    token_value(auth)
        .as_object()
        .expect("token object")
        .iter()
        .all(|(key, value)| root.get("tokens").and_then(|tokens| tokens.get(key)) == Some(value))
}
