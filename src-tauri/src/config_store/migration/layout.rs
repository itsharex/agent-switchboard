use super::super::{
    content_revision, history::HistoryFile, read_optional, ConfigStore, PendingProfileSave,
};
use crate::config_store::snapshot::previous::{previous_defaults, PreviousSnapshot};
use crate::config_store::snapshot::{validate_snapshot, ConfigurationSnapshot};
use asb_core::contracts::{AppKind, ProviderFile};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    read_optional(path)
        .map_err(|error| format!("{}：{error}", path.display()))?
        .map(|text| {
            serde_json::from_str(&text).map_err(|error| format!("{}：{error}", path.display()))
        })
        .transpose()
}

fn entries(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("{}：{error}", path.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_symlink()
        {
            return Err(format!(
                "配置升级不接受符号链接：{}",
                entry.path().display()
            ));
        }
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

pub(super) fn provider_values(store: &ConfigStore, app: AppKind) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    for path in entries(&store.providers_dir(app))? {
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json") {
            return Err(format!("配置升级发现未知供应商材料：{}", path.display()));
        }
        let value: Value = read_json(&path)?.ok_or_else(|| "供应商文件消失".to_string())?;
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("供应商文件缺少标识：{}", path.display()))?;
        if uuid::Uuid::parse_str(id).is_err()
            || path.file_stem().and_then(|name| name.to_str()) != Some(id)
        {
            return Err(format!("供应商文件标识不匹配：{}", path.display()));
        }
        values.push(value);
    }
    values.sort_by_key(|value| value.get("position").and_then(Value::as_u64));
    Ok(values)
}

pub(super) fn needs_upgrade(store: &ConfigStore) -> Result<bool, String> {
    let common = store.configuration_dir().join("common").exists();
    let client = store.configuration_dir().join("client-settings").exists();
    let mut previous = false;
    let mut current = false;
    for app in [AppKind::Codex, AppKind::Claude] {
        for value in provider_values(store, app)? {
            if value.get("parameters").is_some() {
                current = true;
            } else {
                previous = true;
            }
        }
    }
    if (common && (client || current)) || (previous && (current || client)) {
        return Err("配置目录混合了升级前后的参数结构，原数据保持不变".to_string());
    }
    Ok(common || previous)
}

pub(super) fn verify_layout(store: &ConfigStore, settings_directory: &str) -> Result<(), String> {
    for path in entries(&store.configuration_dir())? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        match name {
            "providers" if path.is_dir() => {
                for client in entries(&path)? {
                    if !client.is_dir()
                        || !matches!(
                            client.file_name().and_then(|name| name.to_str()),
                            Some("codex" | "claude")
                        )
                    {
                        return Err(format!("未知供应商分组：{}", client.display()));
                    }
                }
            }
            name if (name == settings_directory || name == "history") && path.is_dir() => {
                for file in entries(&path)? {
                    if !file.is_file()
                        || !matches!(
                            file.file_name().and_then(|name| name.to_str()),
                            Some("codex.json" | "claude.json")
                        )
                    {
                        return Err(format!("未知客户端配置材料：{}", file.display()));
                    }
                }
            }
            "save-journal.json"
            | super::super::SWITCH_INTENT_FILE
            | super::super::PROFILE_PREIMAGE_FILE
                if path.is_file() => {}
            _ => return Err(format!("配置升级发现未知材料：{}", path.display())),
        }
    }
    Ok(())
}

pub(super) fn read_previous(store: &ConfigStore) -> Result<ConfigurationSnapshot, String> {
    verify_layout(store, "common")?;
    let mut snapshot = PreviousSnapshot {
        schema_version: 3,
        providers: BTreeMap::new(),
        common: BTreeMap::new(),
        history: BTreeMap::new(),
    };
    for app in [AppKind::Codex, AppKind::Claude] {
        snapshot.providers.insert(app, provider_values(store, app)?);
        let common_path = store
            .configuration_dir()
            .join("common")
            .join(format!("{}.json", app.dir_name()));
        snapshot.common.insert(
            app,
            read_json(&common_path)?.unwrap_or_else(|| previous_defaults(app)),
        );
        let history: Option<HistoryFile> = read_json(&store.history_path(app))?;
        snapshot
            .history
            .insert(app, history.map(|file| file.records).unwrap_or_default());
    }
    snapshot.into_current()
}

pub(super) fn pending_save(store: &ConfigStore) -> Result<Option<PendingProfileSave>, String> {
    let pending: Option<PendingProfileSave> = read_json(&store.profile_save_journal_path())?;
    let Some(pending) = pending else {
        return Ok(None);
    };
    if uuid::Uuid::parse_str(&pending.profile_id).is_err() || pending.previous_file_hash.is_empty()
    {
        return Err("供应商保存恢复记录不完整".to_string());
    }
    let path = store
        .providers_dir(pending.app)
        .join(format!("{}.json", pending.profile_id));
    let text = fs::read(&path).map_err(|error| format!("{}：{error}", path.display()))?;
    if content_revision(&text) == pending.previous_file_hash {
        Ok(None)
    } else {
        Ok(Some(pending))
    }
}

pub(super) fn validate_current(store: &ConfigStore) -> Result<(), String> {
    if store.configuration_dir().join("common").exists() {
        return Err("升级恢复目标仍包含旧通用设置".to_string());
    }
    let mut snapshot = ConfigurationSnapshot {
        schema_version: super::super::snapshot::CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
        providers: BTreeMap::new(),
        client_settings: BTreeMap::new(),
        history: BTreeMap::new(),
    };
    for app in [AppKind::Codex, AppKind::Claude] {
        let files = provider_values(store, app)?
            .into_iter()
            .map(serde_json::from_value::<ProviderFile>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        snapshot.providers.insert(app, files);
        snapshot.client_settings.insert(
            app,
            read_json(&store.client_settings_path(app))?
                .ok_or_else(|| "升级后的客户端设置缺失".to_string())?,
        );
        let history: HistoryFile = read_json(&store.history_path(app))?
            .ok_or_else(|| "升级后的写入历史缺失".to_string())?;
        snapshot.history.insert(app, history.records);
    }
    validate_snapshot(&snapshot)?;
    if let Some(pending) = read_json::<PendingProfileSave>(&store.profile_save_journal_path())? {
        if pending.previous_file_hash.is_empty()
            || !snapshot.providers[&pending.app]
                .iter()
                .any(|file| file.id == pending.profile_id)
        {
            return Err("升级后的供应商保存恢复记录与档案不匹配".to_string());
        }
    }
    Ok(())
}

pub(super) fn fingerprint(root: &Path) -> Result<Vec<u8>, String> {
    fn add(path: &Path, root: &Path, hash: &mut Sha256) -> Result<(), String> {
        for child in entries(path)? {
            hash.update(
                child
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .as_bytes(),
            );
            if child.is_dir() {
                add(&child, root, hash)?;
            } else {
                let bytes = fs::read(&child).map_err(|error| error.to_string())?;
                hash.update(bytes.len().to_le_bytes());
                hash.update(bytes);
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    add(root, root, &mut hash)?;
    Ok(hash.finalize().to_vec())
}
