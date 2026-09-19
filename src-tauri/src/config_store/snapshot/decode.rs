use super::{validate_snapshot, ConfigurationSnapshot, CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION};

pub(super) const UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT: &str = "云端备份不是当前支持的配置数据格式";

pub(crate) fn decode_cloud_backup_snapshot(
    cleartext: &[u8],
) -> Result<ConfigurationSnapshot, String> {
    let value: serde_json::Value = serde_json::from_slice(cleartext)
        .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64);
    let snapshot = match version {
        Some(version) if version == u64::from(CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION) => {
            serde_json::from_value(value)
                .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?
        }
        _ => return Err("云端备份配置快照版本不受支持".to_string()),
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}
