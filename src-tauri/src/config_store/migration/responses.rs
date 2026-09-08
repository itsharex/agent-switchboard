//! One-time conversion of application-owned Responses transport settings.
use super::{layout, subagent_parameters, ConfigStore};
use crate::config_store::snapshot::{
    validate_snapshot, ConfigurationSnapshot, CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
};
use asb_core::contracts::{AppKind, ProviderFile};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn needs_upgrade(store: &ConfigStore) -> Result<bool, String> {
    let mut previous = false;
    let mut current = false;
    for app in [AppKind::Codex, AppKind::Claude] {
        for value in layout::provider_values(store, app)? {
            if let Some(options) = value.get("responsesOptions").filter(|v| !v.is_null()) {
                if options.get("supportsWebsockets").is_some() {
                    previous = true;
                } else {
                    current = true;
                }
            }
        }
    }
    if previous && current {
        return Err("配置目录混合了升级前后的 Responses 结构，原数据保持不变".into());
    }
    Ok(previous)
}

pub(crate) fn convert_options(value: &mut Value) -> Result<(), String> {
    let Some(options) = value
        .get_mut("responsesOptions")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    if let Some(previous) = options.remove("supportsWebsockets") {
        if !previous.is_boolean() {
            return Err("旧 Responses WebSocket 配置不是布尔值，已取消升级".into());
        }
    }
    Ok(())
}

pub(super) fn read_previous(store: &ConfigStore) -> Result<ConfigurationSnapshot, String> {
    layout::verify_layout(store, "client-settings")?;
    let mut snapshot = ConfigurationSnapshot {
        schema_version: CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
        providers: BTreeMap::new(),
        client_settings: BTreeMap::new(),
        history: BTreeMap::new(),
    };
    for app in [AppKind::Codex, AppKind::Claude] {
        let providers = layout::provider_values(store, app)?
            .into_iter()
            .map(|mut value| {
                convert_options(&mut value)?;
                subagent_parameters::add_defaults(app, &mut value)?;
                serde_json::from_value::<ProviderFile>(value).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        snapshot.providers.insert(app, providers);
        snapshot.client_settings.insert(
            app,
            store
                .get_client_settings(app)
                .map_err(|e| e.to_string())?
                .settings,
        );
        snapshot
            .history
            .insert(app, store.load_history(app).map_err(|e| e.to_string())?);
    }
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}
