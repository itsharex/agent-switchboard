//! Marker writes have no application metadata; verified executor snapshots are sufficient.
use super::{AppKind, ClaudeIntegrationFlag, FsIo, Path, PathBuf, BACKUP_DIR};
use asb_switch::{finish_config_recovery, pending_config_write, sha256_hex, PendingConfigWrite};

pub(super) fn recover(root: &Path) -> Result<(), String> {
    let directory = root.join(BACKUP_DIR);
    let Some(pending) = pending_config_write(&FsIo, &directory, AppKind::Claude)
        .map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let result = recover_pending(&directory, &pending);
    result.map_err(|error| format!(
        "Claude 客户端集成事务未完成：{error}；已保留事务 {}、备份 {}。请保留这些文件并核对目标 {} 和写入锁 {} 后重试集成标记操作",
        asb_switch::config_journal_path(&directory, AppKind::Claude).display(),
        pending.backup.backup_path,
        pending.backup.target_path,
        asb_switch::lock_path_for(Path::new(&pending.backup.target_path)).display(),
    ))
}

fn recover_pending(directory: &Path, pending: &PendingConfigWrite) -> Result<(), String> {
    let flag = match pending.backup.reason.as_str() {
        "claude-integration-plugin" => ClaudeIntegrationFlag::Plugin,
        "claude-integration-onboarding" => ClaudeIntegrationFlag::Onboarding,
        _ => return Err("事务不属于 Claude 客户端集成标记".into()),
    };
    let target = flag.target()?;
    validate_owner(pending, &target)?;
    let backup = std::fs::read_to_string(&pending.backup.backup_path)
        .map_err(|_| "集成标记备份不可读")?;
    if sha256_hex(&backup) != pending.backup.content_hash {
        return Err("集成标记备份哈希不匹配".into());
    }
    let (current, existed) = match std::fs::read_to_string(&target) {
        Ok(text) => (text, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ("{}".into(), false),
        Err(_) => return Err("集成标记文件不可读".into()),
    };
    let hash = sha256_hex(&current);
    if !matches_snapshot(pending, &hash, existed) {
        return Err("集成标记文件已发生额外变化，未覆盖当前内容".into());
    }
    finish_config_recovery(&FsIo, directory, pending, &hash, existed, || Ok(()))
        .map_err(|error| error.to_string())
}

fn validate_owner(pending: &PendingConfigWrite, target: &Path) -> Result<(), String> {
    if pending.app != AppKind::Claude || pending.profile_id.is_some() || pending.auth.is_some()
        || PathBuf::from(&pending.backup.target_path) != target || !pending.after_existed
    {
        return Err("集成标记事务的目标或写入范围不匹配".into());
    }
    Ok(())
}

fn matches_snapshot(pending: &PendingConfigWrite, hash: &str, existed: bool) -> bool {
    (hash == pending.backup.content_hash && existed == pending.backup.target_existed)
        || (hash == pending.after_hash && existed == pending.after_existed)
}
