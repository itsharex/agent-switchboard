//! The port journal owns the transaction; executor records own each file write.
use super::*;
use asb_switch::{finish_config_recovery, pending_config_write, PendingConfigWrite};
use asb_switch::io::FsIo;

pub(super) fn reconcile(
    local: &LocalState,
    journal: &PortChangeJournal,
    keep_external: bool,
) -> Result<(), String> {
    for client in &journal.clients {
        let directory = local.backup_dir();
        let Some(pending) = pending_config_write(&FsIo, &directory, client.app)
            .map_err(|error| error.to_string())? else { continue };
        let target = local.target(client.app)?;
        validate_owner(&pending, client, &target)?;
        let bytes = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && keep_external => None,
            Err(error) => return Err(format!("无法读取端口事务恢复目标：{error}")),
        };
        let empty: &[u8] = match client.app {
            AppKind::Codex => b"",
            AppKind::Claude => b"{}",
        };
        let hash = hash_bytes(bytes.as_deref().unwrap_or(empty));
        if !keep_external && hash != client.before_hash && hash != client.after_hash {
            return Err("客户端配置在事务外被修改，保留端口与执行器恢复记录".into());
        }
        finish_config_recovery(&FsIo, &directory, &pending, &hash, bytes.is_some(), || Ok(()))
            .map_err(|error| format!("无法完成端口文件执行器恢复：{error}"))?;
    }
    Ok(())
}

fn validate_owner(
    pending: &PendingConfigWrite,
    client: &JournalClient,
    target: &Path,
) -> Result<(), String> {
    let (before, after) = match pending.backup.reason.as_str() {
        "gateway-port-change" => (&client.before_hash, &client.after_hash),
        "gateway-port-rollback" => (&client.after_hash, &client.before_hash),
        _ => return Err("存在不属于端口修改的客户端事务，请先恢复该事务".into()),
    };
    if Path::new(&pending.backup.target_path) != target
        || pending.profile_id.is_some() || pending.auth.is_some()
        || !pending.backup.target_existed || !pending.after_existed
        || &pending.backup.content_hash != before || &pending.after_hash != after
    {
        return Err("执行器记录与端口修改事务不匹配，已保留两份恢复记录".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(reason: &str) -> (PendingConfigWrite, JournalClient) {
        let client = JournalClient {
            app: AppKind::Codex,
            before_hash: "before".into(), after_hash: "after".into(),
            backup_file: "codex.toml".into(), history: None,
        };
        let (before, after) = if reason == "gateway-port-rollback" {
            ("after", "before")
        } else { ("before", "after") };
        let pending = serde_json::from_value(serde_json::json!({
            "version":1, "app":"codex", "profileId":null,
            "backup":{
                "id":"backup", "app":"codex", "targetPath":"config.toml",
                "backupPath":"backup.toml", "createdAt":"2026-09-20T00:00:00Z",
                "contentHash":before, "targetExisted":true,
                "linkedBackupId":null, "reason":reason,
            },
            "afterHash":after, "afterExisted":true, "auth":null,
        })).unwrap();
        (pending, client)
    }

    #[test]
    fn port_recovery_accepts_both_directions_but_not_another_transaction() {
        for reason in ["gateway-port-change", "gateway-port-rollback"] {
            let (mut pending, client) = records(reason);
            assert!(validate_owner(&pending, &client, Path::new("config.toml")).is_ok());
            pending.after_hash = "unrelated".into();
            assert!(validate_owner(&pending, &client, Path::new("config.toml")).is_err());
        }
        let (pending, client) = records("manual-edit");
        assert!(validate_owner(&pending, &client, Path::new("config.toml")).is_err());
    }
}
