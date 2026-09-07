//! Whole-configuration snapshot: the unit encrypted for cloud backup and
//! verified before it is enabled. The snapshot is data only — enabling it is
//! a staging write plus a directory swap owned by this module.

mod activation;
mod legacy;

#[cfg(test)]
mod tests;

pub use activation::enable_snapshot;
pub(crate) use legacy::decode_cloud_backup_snapshot;

use super::{history, providers, ConfigStore, ProfileStoreError};
use asb_core::contracts::{AppKind, CommonSettings, ConfigWriteRecord, ProviderFile, RouteMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The only plaintext configuration snapshot format written to cloud backup.
/// The AES-GCM envelope has its own version and is intentionally independent.
pub const CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION: u8 = 3;

/// The complete persisted configuration of both clients. Provider files are
/// grouped by the directory that owns their client association; secrets stay
/// inside the provider files exactly as on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationSnapshot {
    pub schema_version: u8,
    pub providers: BTreeMap<AppKind, Vec<ProviderFile>>,
    pub common: BTreeMap<AppKind, CommonSettings>,
    pub history: BTreeMap<AppKind, Vec<ConfigWriteRecord>>,
}

impl ConfigurationSnapshot {
    pub fn provider_count(&self) -> usize {
        self.providers.values().map(Vec::len).sum()
    }
}

/// Reads the current on-disk configuration as a snapshot. Every file passes
/// the same strict validation a runtime read would apply.
pub fn read_configuration_snapshot(
    store: &ConfigStore,
) -> Result<ConfigurationSnapshot, ProfileStoreError> {
    let mut providers = BTreeMap::new();
    let mut common = BTreeMap::new();
    let mut history_map = BTreeMap::new();
    for app in [AppKind::Codex, AppKind::Claude] {
        providers.insert(app, providers::load_provider_files(store, app)?);
        common.insert(app, store.get_common_settings(app)?.settings);
        history_map.insert(app, store.load_history(app)?);
    }
    Ok(ConfigurationSnapshot {
        schema_version: CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
        providers,
        common,
        history: history_map,
    })
}

/// Validates a snapshot without touching the filesystem: UUID identifiers,
/// provider contracts, common-settings completeness, and history records.
pub fn validate_snapshot(snapshot: &ConfigurationSnapshot) -> Result<(), String> {
    if snapshot.schema_version != CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION {
        return Err("云端备份配置快照版本不受支持".to_string());
    }
    let mut seen_ids = std::collections::HashSet::new();
    for app in [AppKind::Codex, AppKind::Claude] {
        let files = snapshot
            .providers
            .get(&app)
            .ok_or_else(|| format!("快照缺少 {app:?} 供应商分组"))?;
        let mut previous_position = 0;
        let mut has_official_route = false;
        for file in files {
            if uuid::Uuid::parse_str(&file.id).is_err() || !seen_ids.insert(file.id.clone()) {
                return Err(format!("供应商标识无效或重复：{}", file.id));
            }
            if file.position <= previous_position {
                return Err(format!("供应商排序位置无效：{}", file.name));
            }
            previous_position = file.position;
            file.clone()
                .into_profile(app)
                .validate()
                .map_err(|error| error.to_string())?;
            if file.route_mode == RouteMode::Official {
                if has_official_route {
                    return Err(format!("{app:?} 存在重复的官方登录入口"));
                }
                has_official_route = true;
            }
        }
        let settings = snapshot
            .common
            .get(&app)
            .ok_or_else(|| format!("快照缺少 {app:?} 通用设置"))?;
        settings
            .validate_for(app)
            .map_err(|error| error.to_string())?;
        for record in snapshot
            .history
            .get(&app)
            .ok_or_else(|| format!("快照缺少 {app:?} 写入历史"))?
        {
            history::validate_write_record(app, record)?;
        }
    }
    Ok(())
}
