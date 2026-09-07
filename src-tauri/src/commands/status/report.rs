use super::ConfigFileStatus;
use crate::commands::error::{store_error, CommandError};
use asb_core::adapter;
use asb_core::contracts::{
    AppKind, ConfigWriteRecord, MatchStatus, ProviderProfile, SwitchPlan, WriteOperation,
};
use asb_switch::io::{FsIo, SwitchIo};
use asb_switch::sha256_hex;
use std::io::ErrorKind;

pub(super) fn classify_match_status(
    last: Option<&ConfigWriteRecord>,
    current_hash: &str,
    matching_profile: Option<&ProviderProfile>,
) -> MatchStatus {
    if let Some(record) = last {
        if record.content_hash == current_hash
            && record.profile_id.is_none()
            && record.operation == WriteOperation::Restore
        {
            return MatchStatus::RestoredBackup {
                at: record.at.clone(),
            };
        }
    }
    if let Some(profile) = matching_profile {
        return MatchStatus::MatchesProfile {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
        };
    }

    match last {
        Some(record) if record.content_hash == current_hash => MatchStatus::ProfileChanged {
            profile_name: record
                .profile_name
                .clone()
                .unwrap_or_else(|| "通用设置".to_string()),
        },
        Some(record) => MatchStatus::ExternallyModified {
            at: record.at.clone(),
        },
        None => MatchStatus::Unmanaged,
    }
}

/// Decides whether the live content still matches a current profile, a
/// restored backup, or the app's last record. Requires the file to be readable
/// and syntactically valid.
fn match_status_for(
    state: &crate::local_state::LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
    kind: AppKind,
    text: &str,
    codex_auth: Option<&str>,
) -> Result<MatchStatus, CommandError> {
    let configuration = state.configuration();
    let hash = sha256_hex(text);
    let last = configuration
        .latest_config_write(kind)
        .map_err(store_error)?;

    let common = configuration
        .get_common_settings(kind)
        .map_err(store_error)?
        .settings;
    let profiles = configuration
        .list_providers()
        .map_err(store_error)?
        .into_iter()
        .filter(|record| record.profile.app == kind)
        .collect::<Vec<_>>();
    let gateway_profile_id = gateway
        .map(|gateway| gateway.active_profile_id(kind, text, codex_auth))
        .transpose()
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
        .flatten();
    let matching_profile = gateway_profile_id
        .as_deref()
        .and_then(|id| profiles.iter().find(|record| record.profile.id == id))
        .or_else(|| {
            profiles.iter().find(|record| {
                let plan = SwitchPlan::direct(record.profile.clone(), common.clone());
                let unchanged = asb_core::validate_plan(&plan.profile, &plan.common).is_ok()
                    && matches!(
                            adapter::preview(text, &plan, ""),
                            Ok(preview) if preview.changes.is_empty()
                    );
                unchanged
            })
        })
        .map(|record| record.profile.clone());

    Ok(classify_match_status(
        last.as_ref(),
        &hash,
        matching_profile.as_ref(),
    ))
}

pub(super) fn active_profile_id(
    profiles: &[ProviderProfile],
    gateway: Option<&crate::gateway::GatewayController>,
    kind: AppKind,
    text: &str,
    codex_auth: Option<&str>,
    last: Option<&ConfigWriteRecord>,
) -> Result<Option<String>, CommandError> {
    if let Some(profile_id) = gateway
        .map(|gateway| gateway.active_profile_id(kind, text, codex_auth))
        .transpose()
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
        .flatten()
    {
        return Ok(Some(profile_id));
    }
    let mut candidates = Vec::new();
    for profile in profiles.iter().filter(|profile| profile.app == kind) {
        let plan = SwitchPlan::direct(
            profile.clone(),
            asb_core::ownership::default_common_settings(kind),
        );
        if adapter::matches_provider_identity(text, codex_auth, &plan).map_err(|error| {
            CommandError::new("provider-identity-unavailable", error.to_string())
        })? {
            candidates.push(profile.id.clone());
        }
    }
    if candidates.len() == 1 {
        return Ok(candidates.pop());
    }
    // Identical connection profiles cannot be distinguished by model or list order.
    Ok(last
        .and_then(|record| record.profile_id.as_ref())
        .filter(|id| candidates.contains(id))
        .cloned())
}

/// The logic owner behind the `config_status` command. Also consumed by the
/// tray, which rebuilds its menu labels outside the command pipeline.
pub(crate) fn config_status_report(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
) -> Result<Vec<ConfigFileStatus>, CommandError> {
    let io = FsIo;
    let profiles = state
        .configuration()
        .list_providers()
        .map_err(store_error)?
        .into_iter()
        .map(|record| record.profile)
        .collect::<Vec<_>>();
    let auth_path = crate::local_state::LocalState::codex_auth_path()
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let codex_auth = match io.read_file(&auth_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => {
            return Err(CommandError::new(
                "provider-identity-unavailable",
                "无法读取 Codex 登录缓存",
            ))
        }
    };
    [AppKind::Codex, AppKind::Claude]
        .into_iter()
        .map(|kind| {
            let target = state
                .target(kind)
                .map_err(|error| CommandError::new("config-path-unavailable", error))?;
            let last_switch = state
                .configuration()
                .latest_config_write(kind)
                .map_err(store_error)?;
            let status = match io.read_file(&target) {
                Ok(text) => match adapter::validate_syntax(kind, &text) {
                    Ok(()) => ConfigFileStatus {
                        app: kind,
                        path: target.to_string_lossy().to_string(),
                        exists: true,
                        syntax_ok: true,
                        route: Some(adapter::route_state(kind, &text)),
                        read_error: None,
                        match_status: match_status_for(
                            state,
                            Some(gateway),
                            kind,
                            &text,
                            codex_auth.as_deref(),
                        )?,
                        active_profile_id: active_profile_id(
                            &profiles,
                            Some(gateway),
                            kind,
                            &text,
                            codex_auth.as_deref(),
                            last_switch.as_ref(),
                        )?,
                        last_switch,
                    },
                    Err(_) => ConfigFileStatus {
                        app: kind,
                        path: target.to_string_lossy().to_string(),
                        exists: true,
                        syntax_ok: false,
                        route: None,
                        read_error: None,
                        match_status: MatchStatus::Unknown,
                        active_profile_id: None,
                        last_switch,
                    },
                },
                Err(error) if error.kind() == ErrorKind::NotFound => ConfigFileStatus {
                    app: kind,
                    path: target.to_string_lossy().to_string(),
                    exists: false,
                    syntax_ok: false,
                    route: None,
                    read_error: None,
                    match_status: MatchStatus::Unknown,
                    active_profile_id: None,
                    last_switch,
                },
                Err(_) => ConfigFileStatus {
                    app: kind,
                    path: target.to_string_lossy().to_string(),
                    exists: true,
                    syntax_ok: false,
                    route: None,
                    read_error: Some("无法读取配置文件".to_string()),
                    match_status: MatchStatus::Unknown,
                    active_profile_id: None,
                    last_switch,
                },
            };
            Ok(status)
        })
        .collect()
}
