use super::{validate_snapshot, ConfigurationSnapshot, CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION};
use asb_core::contracts::{
    AppKind, CommonSettings, ConfigWriteRecord, ExplicitMaxOutputTokens, ModelOptions,
    ProviderFile, RouteMode, UpstreamProtocol, UsageQuery,
};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) const UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT: &str = "云端备份不是当前支持的配置数据格式";
/// A decoded cloud snapshot. Callers must immediately replace the remote
/// payload whenever `migrated` is true, leaving only the current schema there.
#[derive(Debug)]
pub(crate) struct DecodedCloudBackupSnapshot {
    pub snapshot: ConfigurationSnapshot,
    pub migrated: bool,
}

/// Decodes the current cloud snapshot schema or one of the two exact
/// historical schemas. Migrations live beside the snapshot owner and rewrite
/// the remote payload immediately, so the rest of the application consumes
/// only the current provider contract.
pub(crate) fn decode_cloud_backup_snapshot(
    cleartext: &[u8],
) -> Result<DecodedCloudBackupSnapshot, String> {
    let value: serde_json::Value = serde_json::from_slice(cleartext)
        .map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let fields = value
        .as_object()
        .ok_or_else(|| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let schema_version = match fields.get("schemaVersion") {
        Some(value) => Some(
            value
                .as_u64()
                .ok_or_else(|| "云端备份配置快照版本不受支持".to_string())?,
        ),
        None => None,
    };

    match schema_version {
        Some(version) if version == u64::from(CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION) => {
            return decode_current_snapshot(value, false);
        }
        Some(2) => return decode_v2_snapshot(value),
        Some(_) => return Err("云端备份配置快照版本不受支持".to_string()),
        None => {}
    }

    let mut unversioned_current = value.clone();
    unversioned_current
        .as_object_mut()
        .expect("cloud backup object was checked above")
        .insert(
            "schemaVersion".to_string(),
            serde_json::Value::from(CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION),
        );
    if let Ok(decoded) = decode_current_snapshot(unversioned_current, true) {
        return Ok(decoded);
    }

    let mut unversioned_v2 = value.clone();
    unversioned_v2
        .as_object_mut()
        .expect("cloud backup object was checked above")
        .insert("schemaVersion".to_string(), serde_json::Value::from(2));
    if let Ok(decoded) = decode_v2_snapshot(unversioned_v2) {
        return Ok(decoded);
    }

    let legacy: LegacyConfigurationSnapshotV1 =
        serde_json::from_value(value).map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let snapshot = legacy.into_current();
    validate_snapshot(&snapshot)?;
    Ok(DecodedCloudBackupSnapshot {
        snapshot,
        migrated: true,
    })
}

fn decode_current_snapshot(
    value: serde_json::Value,
    migrated: bool,
) -> Result<DecodedCloudBackupSnapshot, String> {
    let snapshot: ConfigurationSnapshot =
        serde_json::from_value(value).map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    validate_snapshot(&snapshot)?;
    Ok(DecodedCloudBackupSnapshot { snapshot, migrated })
}

fn decode_v2_snapshot(value: serde_json::Value) -> Result<DecodedCloudBackupSnapshot, String> {
    let legacy: LegacyConfigurationSnapshotV2 =
        serde_json::from_value(value).map_err(|_| UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT.to_string())?;
    let snapshot = legacy.into_current();
    validate_snapshot(&snapshot)?;
    Ok(DecodedCloudBackupSnapshot {
        snapshot,
        migrated: true,
    })
}

/// Exact provider-file shape written by snapshot schema version 2. Its
/// explicit authentication value is deliberately discarded: version 3 derives
/// credential delivery from `upstreamProtocol`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyConfigurationSnapshotV2 {
    #[serde(rename = "schemaVersion")]
    _schema_version: u8,
    providers: BTreeMap<AppKind, Vec<LegacyProviderFileV2>>,
    common: BTreeMap<AppKind, CommonSettings>,
    history: BTreeMap<AppKind, Vec<ConfigWriteRecord>>,
}

impl LegacyConfigurationSnapshotV2 {
    fn into_current(self) -> ConfigurationSnapshot {
        ConfigurationSnapshot {
            schema_version: CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
            providers: self
                .providers
                .into_iter()
                .map(|(app, files)| {
                    (
                        app,
                        files
                            .into_iter()
                            .map(LegacyProviderFileV2::into_current)
                            .collect(),
                    )
                })
                .collect(),
            common: self.common,
            history: self.history,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyProviderFileV2 {
    id: String,
    name: String,
    position: u64,
    route_mode: RouteMode,
    api_key: String,
    upstream_protocol: Option<UpstreamProtocol>,
    #[serde(rename = "authScheme")]
    _auth_scheme: LegacyAuthenticationSchemeV2,
    max_output_tokens: ExplicitMaxOutputTokens,
    base_url: Option<String>,
    model: Option<String>,
    model_options: Option<ModelOptions>,
    notes: Option<String>,
    website_url: Option<String>,
    usage_query: Option<UsageQuery>,
}

impl LegacyProviderFileV2 {
    fn into_current(self) -> ProviderFile {
        ProviderFile {
            id: self.id,
            name: self.name,
            position: self.position,
            route_mode: self.route_mode,
            api_key: self.api_key,
            upstream_protocol: self.upstream_protocol,
            max_output_tokens: self.max_output_tokens,
            base_url: self.base_url,
            model: self.model,
            model_options: self.model_options,
            notes: self.notes,
            website_url: self.website_url,
            usage_query: self.usage_query,
            official_quota_refresh_interval_minutes: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum LegacyAuthenticationSchemeV2 {
    None,
    Bearer,
    XApiKey,
}

/// Exact pre-protocol cloud snapshot shape. It is accepted only to make one
/// deterministic migration for encrypted backups that already belong to the
/// user; it is never written again.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyConfigurationSnapshotV1 {
    providers: BTreeMap<AppKind, Vec<LegacyProviderFileV1>>,
    common: BTreeMap<AppKind, CommonSettings>,
    history: BTreeMap<AppKind, Vec<ConfigWriteRecord>>,
}

impl LegacyConfigurationSnapshotV1 {
    fn into_current(self) -> ConfigurationSnapshot {
        let providers = self
            .providers
            .into_iter()
            .map(|(app, files)| {
                (
                    app,
                    files
                        .into_iter()
                        .map(|file| file.into_current(app))
                        .collect(),
                )
            })
            .collect();
        ConfigurationSnapshot {
            schema_version: CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
            providers,
            common: self.common,
            history: self.history,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyProviderFileV1 {
    id: String,
    name: String,
    position: u64,
    route_mode: RouteMode,
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
    model_options: Option<ModelOptions>,
    notes: Option<String>,
    website_url: Option<String>,
    usage_query: Option<UsageQuery>,
}

impl LegacyProviderFileV1 {
    fn into_current(self, app: AppKind) -> ProviderFile {
        let upstream_protocol = match self.route_mode {
            RouteMode::Official => None,
            RouteMode::Custom => Some(match app {
                AppKind::Codex => UpstreamProtocol::Responses,
                AppKind::Claude => UpstreamProtocol::AnthropicMessages,
            }),
        };
        ProviderFile {
            id: self.id,
            name: self.name,
            position: self.position,
            route_mode: self.route_mode,
            api_key: self.api_key,
            upstream_protocol,
            max_output_tokens: None.into(),
            base_url: self.base_url,
            model: self.model,
            model_options: self.model_options,
            notes: self.notes,
            website_url: self.website_url,
            usage_query: self.usage_query,
            official_quota_refresh_interval_minutes: None,
        }
    }
}
