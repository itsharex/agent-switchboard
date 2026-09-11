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
                .unwrap_or_else(|| "客户端设置".to_string()),
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
pub(super) fn match_status_for(
    state: &crate::local_state::LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
    kind: AppKind,
    text: &str,
) -> Result<MatchStatus, CommandError> {
    let configuration = state.configuration();
    let hash = sha256_hex(text);
    let last = configuration
        .latest_config_write(kind)
        .map_err(store_error)?;

    let client_settings = configuration
        .get_client_settings(kind)
        .map_err(store_error)?
        .settings;
    let profiles = configuration
        .list_providers()
        .map_err(store_error)?
        .into_iter()
        .filter(|record| record.profile.app == kind)
        .collect::<Vec<_>>();
    let gateway_profile_id = gateway
        .map(|gateway| gateway.active_profile_id(kind, text))
        .transpose()
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
        .flatten();
    if kind == AppKind::Codex {
        let matching = codex_matching_profile(state, gateway, text, client_settings)?;
        return Ok(classify_match_status(
            last.as_ref(),
            &hash,
            matching.as_ref(),
        ));
    }
    let mut matching_profile = None;
    for record in &profiles {
        if record.profile.requires_gateway()
            && gateway_profile_id.as_deref() != Some(&record.profile.id)
        {
            continue;
        }
        let mut plan = SwitchPlan::direct(record.profile.clone(), client_settings.clone());
        if gateway_profile_id.as_deref() == Some(&record.profile.id) {
            if let Some(projected) = gateway
                .expect("gateway identity has a controller")
                .active_route_projection(&plan)
                .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
            {
                plan = projected;
            }
        }
        if projection_matches(text, &plan) {
            matching_profile = Some(record.profile.clone());
            break;
        }
    }

    Ok(classify_match_status(
        last.as_ref(),
        &hash,
        matching_profile.as_ref(),
    ))
}

fn codex_matching_profile(
    state: &crate::local_state::LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
    text: &str,
    client_settings: asb_core::contracts::SettingsValues,
) -> Result<Option<ProviderProfile>, CommandError> {
    let Some(gateway) = gateway else {
        return Ok(None);
    };
    let Some(profile_id) = gateway
        .active_profile_id(AppKind::Codex, text)
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
    else {
        return Ok(None);
    };
    let file = state
        .configuration()
        .find_codex_provider_file(&profile_id)
        .map_err(store_error)?;
    let Some(plan) = gateway
        .active_codex_projection(&file, client_settings)
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
    else {
        return Ok(None);
    };
    Ok(projection_matches(text, &plan)
        .then(|| file.client_projection().into_profile(AppKind::Codex)))
}

fn projection_matches(text: &str, plan: &SwitchPlan) -> bool {
    asb_core::validate_plan(&plan.profile, &plan.client_settings).is_ok()
        && matches!(adapter::preview(text, plan, ""), Ok(preview) if preview.changes.is_empty())
}

pub(super) fn active_profile_id(
    profiles: &[ProviderProfile],
    gateway: Option<&crate::gateway::GatewayController>,
    kind: AppKind,
    text: &str,
    last: Option<&ConfigWriteRecord>,
) -> Result<Option<String>, CommandError> {
    if let Some(profile_id) = gateway
        .map(|gateway| gateway.active_profile_id(kind, text))
        .transpose()
        .map_err(|error| CommandError::new("provider-identity-unavailable", error))?
        .flatten()
    {
        return Ok(Some(profile_id));
    }
    let mut candidates = Vec::new();
    for profile in profiles
        .iter()
        .filter(|profile| profile.app == kind && !profile.requires_gateway())
    {
        let plan = SwitchPlan::direct(
            profile.clone(),
            asb_core::ownership::default_client_settings(kind),
        );
        if adapter::matches_provider_identity(text, &plan).map_err(|error| {
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
    let profiles = state
        .configuration()
        .list_providers()
        .map_err(store_error)?
        .into_iter()
        .map(|record| record.profile)
        .collect::<Vec<_>>();
    [AppKind::Codex, AppKind::Claude]
        .into_iter()
        .map(|kind| observe_client_file(state, gateway, &profiles, kind))
        .collect()
}

fn observe_client_file(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profiles: &[ProviderProfile],
    kind: AppKind,
) -> Result<ConfigFileStatus, CommandError> {
    let target = state
        .target(kind)
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let mut status = ConfigFileStatus {
        app: kind,
        path: target.to_string_lossy().to_string(),
        exists: false,
        syntax_ok: false,
        route: None,
        read_error: None,
        match_status: MatchStatus::Unknown,
        active_profile_id: None,
        last_switch: state
            .configuration()
            .latest_config_write(kind)
            .map_err(store_error)?,
    };
    let text = match FsIo.read_file(&target) {
        Ok(text) => {
            status.exists = true;
            text
        }
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(status),
        Err(_) => {
            status.exists = true;
            status.read_error = Some("无法读取配置文件".to_string());
            return Ok(status);
        }
    };
    if adapter::validate_syntax(kind, &text).is_err() {
        return Ok(status);
    }
    status.syntax_ok = true;
    let mut route = adapter::route_state(kind, &text);
    if kind == AppKind::Codex {
        route.base_url = route
            .base_url
            .map(|url| asb_core::redact::redact("openai_base_url", &url));
    }
    status.route = Some(route);
    status.match_status = match_status_for(state, Some(gateway), kind, &text)?;
    status.active_profile_id = if kind == AppKind::Codex {
        let settings = state
            .configuration()
            .get_client_settings(AppKind::Codex)
            .map_err(store_error)?
            .settings;
        match codex_matching_profile(state, Some(gateway), &text, settings)? {
            Some(profile) => Some(profile.id),
            // A third-party route never reaches here; official routing falls
            // back to the stored official-login record so the row can show
            // its live state like Claude official profiles do.
            None => active_profile_id(
                profiles,
                Some(gateway),
                kind,
                &text,
                status.last_switch.as_ref(),
            )?,
        }
    } else {
        active_profile_id(
            profiles,
            Some(gateway),
            kind,
            &text,
            status.last_switch.as_ref(),
        )?
    };
    Ok(status)
}
