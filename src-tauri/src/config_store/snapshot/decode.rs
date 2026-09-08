use super::{
    previous::PreviousSnapshot, validate_snapshot, ConfigurationSnapshot,
    CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
};

pub(super) const UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT: &str = "云端备份不是当前支持的配置数据格式";

#[derive(Debug)]
pub(crate) struct DecodedCloudBackupSnapshot {
    pub snapshot: ConfigurationSnapshot,
    pub migrated: bool,
}

/// Previous snapshots are converted once at the restore boundary. The restore
/// owner rewrites converted payloads before enabling the current strict format.
pub(crate) fn decode_cloud_backup_snapshot(
    cleartext: &[u8],
) -> Result<DecodedCloudBackupSnapshot, String> {
    let value: serde_json::Value = serde_json::from_slice(cleartext)
        .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64);
    let migrated = matches!(version, Some(3 | 4));
    let snapshot = match version {
        Some(version) if version == u64::from(CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION) => {
            serde_json::from_value(value)
                .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?
        }
        Some(3) => serde_json::from_value::<PreviousSnapshot>(value)
            .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?
            .into_current()?,
        Some(4) => convert_responses_snapshot(value)?,
        _ => return Err("云端备份配置快照版本不受支持".to_string()),
    };
    validate_snapshot(&snapshot)?;
    Ok(DecodedCloudBackupSnapshot { snapshot, migrated })
}

fn convert_responses_snapshot(
    mut value: serde_json::Value,
) -> Result<ConfigurationSnapshot, String> {
    let groups = value
        .get_mut("providers")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    for files in groups.values_mut() {
        let files = files
            .as_array_mut()
            .ok_or_else(|| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
        for file in files {
            crate::config_store::migration::responses::convert_options(file)?;
        }
    }
    value["schemaVersion"] = CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION.into();
    serde_json::from_value(value).map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())
}
