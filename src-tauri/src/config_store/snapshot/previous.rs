//! Exact schema-3 conversion shared by local startup and cloud restoration.
//! The ownership catalog is the only source for partitioning the old values.

use super::{validate_snapshot, ConfigurationSnapshot, CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION};
use asb_core::contracts::{
    AppKind, ConfigWriteRecord, ProviderFile, ResponsesOptions, ResponsesRequestMode, RouteMode,
    SettingValue, SettingsValues, UpstreamProtocol,
};
use asb_core::ownership::{default_client_settings, default_provider_parameters};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreviousSnapshot {
    pub schema_version: u8,
    pub providers: BTreeMap<AppKind, Vec<Value>>,
    pub common: BTreeMap<AppKind, SettingsValues>,
    pub history: BTreeMap<AppKind, Vec<ConfigWriteRecord>>,
}

impl PreviousSnapshot {
    pub(crate) fn into_current(mut self) -> Result<ConfigurationSnapshot, String> {
        if self.schema_version != 3 {
            return Err("仅支持从配置快照版本 3 升级".to_string());
        }
        let mut providers = BTreeMap::new();
        let mut client_settings = BTreeMap::new();
        for app in [AppKind::Codex, AppKind::Claude] {
            let previous = self
                .common
                .remove(&app)
                .ok_or_else(|| format!("旧快照缺少 {app:?} 通用设置"))?;
            let (parameters, client) = partition_settings(app, previous)?;
            let previous_files = self
                .providers
                .remove(&app)
                .ok_or_else(|| format!("旧快照缺少 {app:?} 供应商分组"))?;
            if previous_files.is_empty() {
                if let Some((key, _)) = parameters
                    .settings
                    .iter()
                    .find(|(_, value)| matches!(value, SettingValue::Explicit { .. }))
                {
                    return Err(format!(
                        "{} 没有供应商档案，无法迁移旧通用设置中的显式供应商参数：{key}",
                        app.label()
                    ));
                }
            }
            let files = previous_files
                .into_iter()
                .map(|value| convert_provider(value, &parameters))
                .collect::<Result<Vec<_>, _>>()?;
            providers.insert(app, files);
            client_settings.insert(app, client);
        }
        let current = ConfigurationSnapshot {
            schema_version: CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
            providers,
            client_settings,
            history: self.history,
        };
        validate_snapshot(&current)?;
        Ok(current)
    }
}

pub(crate) fn previous_defaults(app: AppKind) -> SettingsValues {
    let mut settings = default_client_settings(app);
    settings
        .settings
        .extend(default_provider_parameters(app).settings);
    settings
}

fn partition_settings(
    app: AppKind,
    mut previous: SettingsValues,
) -> Result<(SettingsValues, SettingsValues), String> {
    let mut parameters = default_provider_parameters(app);
    let mut client = default_client_settings(app);
    for (key, value) in parameters
        .settings
        .iter_mut()
        .chain(client.settings.iter_mut())
    {
        *value = previous
            .settings
            .remove(key)
            .ok_or_else(|| format!("旧通用设置缺少参数：{key}"))?;
    }
    if let Some(key) = previous.settings.keys().next() {
        return Err(format!("旧通用设置含有不支持的参数：{key}"));
    }
    parameters
        .validate_provider_parameters(app)
        .map_err(|error| error.to_string())?;
    client
        .validate_client_settings(app)
        .map_err(|error| error.to_string())?;
    Ok((parameters, client))
}

fn convert_provider(mut value: Value, parameters: &SettingsValues) -> Result<ProviderFile, String> {
    crate::config_store::migration::responses::convert_options(&mut value)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "旧供应商文件必须是对象".to_string())?;
    if object.contains_key("parameters") {
        return Err("旧快照混入了新供应商参数结构".to_string());
    }
    object.insert(
        "parameters".to_string(),
        serde_json::to_value(parameters).map_err(|error| error.to_string())?,
    );
    add_previous_responses_mode(object)?;
    serde_json::from_value(value)
        .map_err(|error| format!("旧供应商文件不符合直接前序契约：{error}"))
}

/// The directly preceding schema had one Responses request shape and did not
/// persist a mode. Its only behavior is today's explicit `standard` mode.
/// This conversion runs only while upgrading that schema; runtime readers
/// continue to require an explicit current field.
fn add_previous_responses_mode(object: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let route_mode: RouteMode = serde_json::from_value(
        object
            .get("routeMode")
            .cloned()
            .ok_or_else(|| "旧供应商文件缺少路由模式".to_string())?,
    )
    .map_err(|error| format!("旧供应商文件路由模式无效：{error}"))?;
    let protocol: Option<UpstreamProtocol> = serde_json::from_value(
        object
            .get("upstreamProtocol")
            .cloned()
            .unwrap_or(Value::Null),
    )
    .map_err(|error| format!("旧供应商文件上游格式无效：{error}"))?;
    if route_mode == RouteMode::Custom && protocol == Some(UpstreamProtocol::Responses) {
        object.insert(
            "responsesOptions".to_string(),
            serde_json::to_value(ResponsesOptions {
                request_mode: ResponsesRequestMode::Standard,
            })
            .map_err(|error| error.to_string())?,
        );
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn snapshot_bytes(snapshot: &ConfigurationSnapshot) -> Vec<u8> {
    let mut value = serde_json::to_value(snapshot).unwrap();
    let root = value.as_object_mut().unwrap();
    root.insert("schemaVersion".into(), Value::from(3));
    let mut common = serde_json::Map::new();
    for app in [AppKind::Codex, AppKind::Claude] {
        let mut settings = snapshot.client_settings[&app].clone();
        let parameters = snapshot.providers[&app]
            .first()
            .map(|file| file.parameters.clone())
            .unwrap_or_else(|| default_provider_parameters(app));
        settings.settings.extend(parameters.settings);
        common.insert(
            app.dir_name().into(),
            serde_json::to_value(settings).unwrap(),
        );
    }
    root.remove("clientSettings");
    root.insert("common".into(), Value::Object(common));
    for files in root["providers"].as_object_mut().unwrap().values_mut() {
        for file in files.as_array_mut().unwrap() {
            file.as_object_mut().unwrap().remove("parameters");
        }
    }
    serde_json::to_vec(&value).unwrap()
}
